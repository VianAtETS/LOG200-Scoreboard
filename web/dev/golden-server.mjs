// Serveur local sur les données « golden » du ladder (crates/isolation-ladder/tests/fixtures/golden),
// pour voir la page sans ladder ni Docker. Lance aussi `astro dev`, branché dessus.
//
//   node dev/golden-server.mjs            # page sur http://localhost:3000
//   PORT=8899 node dev/golden-server.mjs  # port du serveur de données
//
// Sert /data (avec Range, comme Caddy), /api/games/<id>, /api/version, /api/health, /api/openapi.json
// (vide), une fausse soumission de jar (PUT /api/submit) et un faux direct /api/live qui rejoue les
// parties golden en boucle, coup par coup. S'y ajoutent un petit
// tournoi, une file d'attente et live.json, absents des fixtures.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream, readFileSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import nodePath from 'node:path';

const PORT = Number(process.env.PORT ?? 8899);
const MOVE_MS = 700;
// Durée du faux smoke test d'une soumission.
const SMOKE_MS = 8000;
const PAUSE_MS = 4000;

const here = import.meta.dirname;
const golden = nodePath.join(here, '../../crates/isolation-ladder/tests/fixtures/golden');
const expected = nodePath.join(golden, 'expected');
const lines = readFileSync(nodePath.join(golden, 'games.jsonl'), 'utf8')
  .split('\n')
  .filter(Boolean);
const games = new Map(
  lines.map((line) => {
    const game = JSON.parse(line);
    return [game.id, line];
  }),
);

const summary = JSON.parse(readFileSync(nodePath.join(expected, 'summary.json'), 'utf8'));
const teams = summary.scopes[''].ratings.flatMap((rating) => (rating.build ? [rating.team] : []));
// Le direct rejoue les parties de la session affichée par défaut : badges de duel au classement.
const current = lines.filter((line) => {
  const game = JSON.parse(line);
  return game.session === summary.sessions[0];
});
const replayed = current.length > 0 ? current : lines;

// --- Données ajoutées : tournoi, file d'attente --------------------------------------------------

const [s1, s2, s3, s4] = teams;
const ids = [...games.keys()];
// oxlint-disable-next-line react-doctor/no-impure-call-at-module-scope -- Tournament timestamps are fixed at server startup.
const ts = new Date(Date.now() - 3 * 36e5).toISOString();
const tournament = {
  v: 1,
  id: 'golden',
  ts,
  session: summary.sessions[0],
  time_ms: 5000,
  seed: 1,
  entrants: [s1, s2, s3, s4].map((team, i) => ({
    team,
    build: summary.builds[team]?.at(-1),
    rating: 1700 - 50 * i,
    multithread: false,
  })),
  game_ms: [60_000, 62_000, 58_000, 61_000],
  slots: [2, 0],
  matches: [
    {
      id: 'W1',
      side: 'winners',
      round: 1,
      best_of: 3,
      head_start: [0, 0],
      from: [{ seed: 1 }, { seed: 4 }],
      teams: [s1, s4],
      score: [2, 0],
      status: 'done',
      winner: s1,
      games: ids.slice(0, 2),
    },
    {
      id: 'W2',
      side: 'winners',
      round: 1,
      best_of: 3,
      head_start: [0, 0],
      from: [{ seed: 2 }, { seed: 3 }],
      teams: [s2, s3],
      score: [2, 1],
      status: 'done',
      winner: s2,
      games: ids.slice(2, 5),
    },
    {
      id: 'W3',
      side: 'winners',
      round: 2,
      best_of: 3,
      head_start: [0, 0],
      from: [{ winner: 'W1' }, { winner: 'W2' }],
      teams: [s1, s2],
      score: [1, 0],
      status: 'running',
      winner: null,
      games: ids.slice(5, 6),
    },
    {
      id: 'L1',
      side: 'losers',
      round: 1,
      best_of: 3,
      head_start: [0, 0],
      from: [{ loser: 'W1' }, { loser: 'W2' }],
      teams: [s4, s3],
      score: [0, 0],
      status: 'ready',
      winner: null,
      games: [],
    },
    {
      id: 'L2',
      side: 'losers',
      round: 2,
      best_of: 3,
      head_start: [0, 0],
      from: [{ winner: 'L1' }, { loser: 'W3' }],
      teams: [null, null],
      score: [0, 0],
      status: 'waiting',
      winner: null,
      games: [],
    },
    {
      id: 'GF',
      side: 'final',
      round: 1,
      best_of: 5,
      head_start: [1, 0],
      from: [{ winner: 'W3' }, { winner: 'L2' }],
      teams: [null, null],
      score: [1, 0],
      status: 'waiting',
      winner: null,
      games: [],
    },
  ],
  ended: null,
};
const index = {
  v: 1,
  // oxlint-disable-next-line react-doctor/no-impure-call-at-module-scope -- The next tournament stays fixed until server restart.
  next: new Date(Date.now() + 7 * 864e5).toISOString(),
  tournaments: [{ id: 'golden', ts, teams: 4, winner: null }],
};

const queue = () => {
  const { ratings } = summary.scopes[''];
  const all = ratings.map((rating, i) => ({
    id: rating.team,
    ahead: i - Math.floor(ratings.length / 2),
    bot: !rating.build,
  }));
  return {
    v: 1,
    teams: all,
    next: [{ teams: [all[0].id, all[1].id], at: new Date(Date.now() + 42_000).toISOString() }],
  };
};

// --- Faux direct : une partie golden à la fois, rejouée en boucle ----------------------------------

const sockets = new Set();
let cycle = 0;
let live = null;

function frame(message) {
  const body = Buffer.from(message);
  let head;
  if (body.length < 126) {
    head = Buffer.from([0x81, body.length]);
  } else if (body.length < 65_536) {
    head = Buffer.alloc(4);
    head[0] = 0x81;
    head[1] = 126;
    head.writeUInt16BE(body.length, 2);
  } else {
    head = Buffer.alloc(10);
    head[0] = 0x81;
    head[1] = 127;
    head.writeBigUInt64BE(BigInt(body.length), 2);
  }
  return Buffer.concat([head, body]);
}

const broadcast = (msg) => {
  const data = frame(JSON.stringify(msg));
  for (const socket of sockets) {
    socket.write(data);
  }
};

function snapshotGame() {
  const { source, n, lastAt, id } = live;
  return {
    id,
    red: source.red.team,
    black: source.black.team,
    since: live.since,
    time_ms: source.time_ms ?? 500,
    opening: source.opening ?? [],
    moves: source.moves.slice(0, n),
    move_ms: (source.move_ms ?? []).slice(0, n),
    turn_ms: Date.now() - lastAt,
  };
}

function start() {
  const source = JSON.parse(replayed[cycle % replayed.length]);
  cycle += 1;
  live = {
    id: `live-${cycle}`,
    source,
    n: 0,
    since: new Date().toISOString(),
    lastAt: Date.now(),
    ended: false,
  };
  broadcast({ v: 1, type: 'started', game: snapshotGame() });
  const timer = setInterval(() => {
    const { source: game } = live;
    if (live.n < game.moves.length) {
      broadcast({
        v: 1,
        type: 'move',
        id: live.id,
        n: live.n,
        move: game.moves[live.n],
        ms: game.move_ms?.[live.n] ?? 100,
      });
      live.n += 1;
      live.lastAt = Date.now();
      return;
    }
    clearInterval(timer);
    live.ended = true;
    broadcast({ v: 1, type: 'ended', id: live.id, winner: game.winner, reason: game.reason });
    setTimeout(start, PAUSE_MS);
  }, MOVE_MS);
}

function upgrade(req, socket) {
  if (new URL(req.url, 'http://x').pathname !== '/api/live') {
    socket.destroy();
    return;
  }
  const accept = createHash('sha1')
    .update(`${req.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
    .digest('base64');
  socket.write(
    `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
  );
  sockets.add(socket);
  const snapshot = { v: 1, type: 'snapshot', games: live && !live.ended ? [snapshotGame()] : [] };
  socket.write(frame(JSON.stringify(snapshot)));
  // La page n'écrit jamais : une trame reçue est la fermeture.
  socket.on('data', () => socket.end());
  socket.on('close', () => sockets.delete(socket));
  socket.on('error', () => sockets.delete(socket));
}

// --- Fausse soumission : comme le ladder, 202 puis verdict sur GET /api/submit/<id> ----------------
// Token vide : 401. Corps qui n'est pas un zip : 400. Token « refuse » : smoke test raté (422).

const types = { '.json': 'application/json', '.jsonl': 'application/x-ndjson' };

function sendJson(res, payload, status = 200) {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(payload);
}

function json(res, body, status = 200) {
  sendJson(res, JSON.stringify(body), status);
}

function text(res, body, status = 200) {
  res.writeHead(status, {
    'Content-Type': 'text/plain; charset=utf-8',
    'Cache-Control': 'no-store',
  });
  res.end(`${body}\n`);
}

const jobs = new Map();

function submit(req, res) {
  const token = /^Bearer (?<token>.+)$/u
    .exec(req.headers.authorization ?? '')
    ?.groups?.token?.trim();
  const chunks = [];
  req.on('data', (chunk) => {
    chunks.push(chunk);
  });
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    if (!token) {
      text(res, "token d'équipe absent ou inconnu (en-tête Authorization: Bearer <token>)", 401);
      return;
    }
    if (body.subarray(0, 2).toString() !== 'PK') {
      text(res, "le corps de la requête n'est pas un jar", 400);
      return;
    }
    const id = Date.now();
    const hash = createHash('sha256').update(body).digest('hex').slice(0, 12);
    jobs.set(String(id), { at: Date.now(), token, hash });
    const path = `/api/submit/${id}`;
    res.writeHead(202, { 'Content-Type': 'text/plain; charset=utf-8', Location: path });
    res.end(`${path}\n`);
  });
}

function verdict(req, res, id) {
  const job = jobs.get(id);
  const token = /^Bearer (?<token>.+)$/u
    .exec(req.headers.authorization ?? '')
    ?.groups?.token?.trim();
  if (!job || job.token !== token) {
    text(res, 'soumission inconnue', 404);
    return;
  }
  if (Date.now() - job.at < SMOKE_MS) {
    text(res, 'en cours : smoke test', 202);
    return;
  }
  if (job.token === 'refuse') {
    text(res, 'refusé : temps dépassé pendant la partie de test contre aleatoire', 422);
    return;
  }
  text(res, `accepté : version ${job.hash}, en jeu dès la prochaine partie`);
}

// --- HTTP ----------------------------------------------------------------------------------------
function file(req, res, path) {
  let st;
  try {
    st = statSync(path);
  } catch {
    res.writeHead(404);
    res.end();
    return;
  }
  const headers = {
    'Content-Type': types[nodePath.extname(path)] ?? 'application/octet-stream',
    'Accept-Ranges': 'bytes',
    'Cache-Control': 'no-store',
  };
  const range = /^bytes=(?<start>\d+)-(?<end>\d*)$/u.exec(req.headers.range ?? '');
  if (range) {
    const rangeStart = Number(range.groups.start);
    const end = range.groups.end ? Number(range.groups.end) : st.size - 1;
    if (rangeStart >= st.size) {
      res.writeHead(416, { 'Content-Range': `bytes */${st.size}` });
      res.end();
      return;
    }
    res.writeHead(206, {
      ...headers,
      'Content-Range': `bytes ${rangeStart}-${end}/${st.size}`,
      'Content-Length': end - rangeStart + 1,
    });
    createReadStream(path, { start: rangeStart, end }).pipe(res);
    return;
  }
  res.writeHead(200, { ...headers, 'Content-Length': st.size });
  createReadStream(path).pipe(res);
}

const server = createServer((req, res) => {
  const pathname = nodePath.normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (pathname === '/api/version') {
    text(res, '0.0.0-golden');
    return;
  }
  if (pathname === '/api/health') {
    text(res, 'ok');
    return;
  }
  if (pathname === '/api/submit' && req.method === 'PUT') {
    submit(req, res);
    return;
  }
  if (pathname.startsWith('/api/submit/')) {
    verdict(req, res, pathname.slice('/api/submit/'.length));
    return;
  }
  if (pathname === '/api/openapi.json') {
    json(res, {
      openapi: '3.1.0',
      info: { title: 'Ladder (golden)', version: '0' },
      paths: {},
    });
    return;
  }
  if (pathname.startsWith('/api/games/')) {
    const game = games.get(pathname.slice('/api/games/'.length));
    if (game) {
      sendJson(res, game);
    } else {
      json(res, 'absent', 404);
    }
    return;
  }
  if (pathname === '/data/live.json') {
    json(res, {
      v: 1,
      games:
        live && !live.ended
          ? [{ id: live.id, red: live.source.red.team, black: live.source.black.team }]
          : [],
    });
    return;
  }
  if (pathname === '/data/queue.json') {
    json(res, queue());
    return;
  }
  if (pathname === '/data/tournaments/index.json') {
    json(res, index);
    return;
  }
  if (pathname === '/data/tournaments/golden.json') {
    json(res, tournament);
    return;
  }
  if (pathname.startsWith('/data/') && !pathname.includes('..')) {
    file(req, res, nodePath.join(expected, pathname.slice('/data/'.length)));
    return;
  }
  res.writeHead(404);
  res.end();
});
server.on('upgrade', (req, socket) => {
  upgrade(req, socket);
});
server.listen(PORT, '127.0.0.1', () => {
  console.log(`Données golden sur http://localhost:${PORT} (/data, /api)`);
  start();
  // --ignore-lock : toujours au premier plan (Astro passe en arrière-plan s'il détecte un agent).
  const astro = spawn(
    process.execPath,
    [
      nodePath.join(here, '../node_modules/astro/bin/astro.mjs'),
      'dev',
      '--ignore-lock',
      ...process.argv.slice(2),
    ],
    {
      cwd: nodePath.join(here, '..'),
      stdio: 'inherit',
      env: { ...process.env, SCOREBOARD_DEV_SERVER: `http://127.0.0.1:${PORT}` },
    },
  );
  astro.on('exit', (code) => {
    // oxlint-disable-next-line unicorn/no-process-exit -- Exit with Astro even while the demo server has active timers.
    process.exit(code ?? 0);
  });
  // Arrêter le serveur arrête aussi astro dev.
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
      astro.kill(signal);
    });
  }
});
