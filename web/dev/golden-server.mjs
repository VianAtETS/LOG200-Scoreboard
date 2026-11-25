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
import { dirname, extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const PORT = Number(process.env.PORT ?? 8899);
const MOVE_MS = 700;
// Durée du faux smoke test d'une soumission.
const SMOKE_MS = 8000;
const PAUSE_MS = 4000;

const here = dirname(fileURLToPath(import.meta.url));
const golden = join(here, '../../crates/isolation-ladder/tests/fixtures/golden');
const expected = join(golden, 'expected');
const lines = readFileSync(join(golden, 'games.jsonl'), 'utf8').split('\n').filter(Boolean);
const games = new Map(lines.map((l) => [JSON.parse(l).id, l]));
const summary = JSON.parse(readFileSync(join(expected, 'summary.json'), 'utf8'));
const teams = summary.scopes[''].ratings.filter((r) => r.build).map((r) => r.team);
// Le direct rejoue les parties de la session affichée par défaut : badges de duel au classement.
const current = lines.filter((l) => JSON.parse(l).session === summary.sessions[0]);
const replayed = current.length ? current : lines;

// --- Données ajoutées : tournoi, file d'attente --------------------------------------------------

const [s1, s2, s3, s4] = teams;
const ids = [...games.keys()];
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
  game_ms: [60000, 62000, 58000, 61000],
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
  next: new Date(Date.now() + 7 * 864e5).toISOString(),
  tournaments: [{ id: 'golden', ts, teams: 4, winner: null }],
};

const queue = () => {
  const all = summary.scopes[''].ratings.map((r) => ({ id: r.team, ahead: 0, bot: !r.build }));
  all.forEach((t, i) => (t.ahead = i - Math.floor(all.length / 2)));
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

function frame(text) {
  const body = Buffer.from(text);
  const head =
    body.length < 126
      ? Buffer.from([0x81, body.length])
      : body.length < 65536
        ? Buffer.from([0x81, 126, body.length >> 8, body.length & 255])
        : Buffer.concat([
            Buffer.from([0x81, 127, 0, 0, 0, 0]),
            Buffer.from([
              (body.length >>> 24) & 255,
              (body.length >>> 16) & 255,
              (body.length >>> 8) & 255,
              body.length & 255,
            ]),
          ]);
  return Buffer.concat([head, body]);
}

const broadcast = (msg) => {
  const data = frame(JSON.stringify(msg));
  for (const s of sockets) s.write(data);
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
  const source = JSON.parse(replayed[cycle++ % replayed.length]);
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
    const { source: g } = live;
    if (live.n < g.moves.length) {
      broadcast({
        v: 1,
        type: 'move',
        id: live.id,
        n: live.n,
        move: g.moves[live.n],
        ms: g.move_ms?.[live.n] ?? 100,
      });
      live.n++;
      live.lastAt = Date.now();
      return;
    }
    clearInterval(timer);
    live.ended = true;
    broadcast({ v: 1, type: 'ended', id: live.id, winner: g.winner, reason: g.reason });
    setTimeout(start, PAUSE_MS);
  }, MOVE_MS);
}

function upgrade(req, socket) {
  if (new URL(req.url, 'http://x').pathname !== '/api/live') return socket.destroy();
  const accept = createHash('sha1')
    .update(`${req.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
    .digest('base64');
  socket.write(
    `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
  );
  sockets.add(socket);
  socket.write(
    frame(
      JSON.stringify({
        v: 1,
        type: 'snapshot',
        games: live && !live.ended ? [snapshotGame()] : [],
      }),
    ),
  );
  // La page n'écrit jamais : une trame reçue est la fermeture.
  socket.on('data', () => socket.end());
  socket.on('close', () => sockets.delete(socket));
  socket.on('error', () => sockets.delete(socket));
}

// --- Fausse soumission : comme le ladder, 202 puis verdict sur GET /api/submit/<id> ----------------
// Token vide : 401. Corps qui n'est pas un zip : 400. Token « refuse » : smoke test raté (422).

const jobs = new Map();

function submit(req, res) {
  const token = /^Bearer (.+)$/.exec(req.headers.authorization ?? '')?.[1]?.trim();
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    if (!token)
      return text(
        res,
        "token d'équipe absent ou inconnu (en-tête Authorization: Bearer <token>)",
        401,
      );
    if (body.subarray(0, 2).toString() !== 'PK')
      return text(res, "le corps de la requête n'est pas un jar", 400);
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
  const token = /^Bearer (.+)$/.exec(req.headers.authorization ?? '')?.[1]?.trim();
  if (!job || job.token !== token) return text(res, 'soumission inconnue', 404);
  if (Date.now() - job.at < SMOKE_MS) return text(res, 'en cours : smoke test', 202);
  if (job.token === 'refuse')
    return text(res, 'refusé : temps dépassé pendant la partie de test contre aleatoire', 422);
  text(res, `accepté : version ${job.hash}, en jeu dès la prochaine partie`);
}

// --- HTTP ----------------------------------------------------------------------------------------

const types = { '.json': 'application/json', '.jsonl': 'application/x-ndjson' };

function json(res, body, status = 200) {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(typeof body === 'string' ? body : JSON.stringify(body));
}

function text(res, body, status = 200) {
  res.writeHead(status, {
    'Content-Type': 'text/plain; charset=utf-8',
    'Cache-Control': 'no-store',
  });
  res.end(`${body}\n`);
}

function file(req, res, path) {
  let st;
  try {
    st = statSync(path);
  } catch {
    res.writeHead(404);
    return res.end();
  }
  const headers = {
    'Content-Type': types[extname(path)] ?? 'application/octet-stream',
    'Accept-Ranges': 'bytes',
    'Cache-Control': 'no-store',
  };
  const range = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range ?? '');
  if (range) {
    const start = Number(range[1]);
    const end = range[2] ? Number(range[2]) : st.size - 1;
    if (start >= st.size) {
      res.writeHead(416, { 'Content-Range': `bytes */${st.size}` });
      return res.end();
    }
    res.writeHead(206, {
      ...headers,
      'Content-Range': `bytes ${start}-${end}/${st.size}`,
      'Content-Length': end - start + 1,
    });
    return createReadStream(path, { start, end }).pipe(res);
  }
  res.writeHead(200, { ...headers, 'Content-Length': st.size });
  createReadStream(path).pipe(res);
}

const server = createServer((req, res) => {
  const p = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (p === '/api/version') return text(res, '0.0.0-golden');
  if (p === '/api/health') return text(res, 'ok');
  if (p === '/api/submit' && req.method === 'PUT') return submit(req, res);
  if (p.startsWith('/api/submit/')) return verdict(req, res, p.slice('/api/submit/'.length));
  if (p === '/api/openapi.json')
    return json(res, {
      openapi: '3.1.0',
      info: { title: 'Ladder (golden)', version: '0' },
      paths: {},
    });
  if (p.startsWith('/api/games/')) {
    const g = games.get(p.slice('/api/games/'.length));
    return g ? json(res, g) : json(res, 'absent', 404);
  }
  if (p === '/data/live.json')
    return json(res, {
      v: 1,
      games:
        live && !live.ended
          ? [{ id: live.id, red: live.source.red.team, black: live.source.black.team }]
          : [],
    });
  if (p === '/data/queue.json') return json(res, queue());
  if (p === '/data/tournaments/index.json') return json(res, index);
  if (p === '/data/tournaments/golden.json') return json(res, tournament);
  if (p.startsWith('/data/') && !p.includes('..'))
    return file(req, res, join(expected, p.slice('/data/'.length)));
  res.writeHead(404);
  res.end();
});
server.on('upgrade', upgrade);
server.listen(PORT, '127.0.0.1', () => {
  console.log(`Données golden sur http://localhost:${PORT} (/data, /api)`);
  start();
  // --ignore-lock : toujours au premier plan (Astro passe en arrière-plan s'il détecte un agent).
  const astro = spawn('npx', ['astro', 'dev', '--ignore-lock', ...process.argv.slice(2)], {
    cwd: join(here, '..'),
    stdio: 'inherit',
    env: { ...process.env, SCOREBOARD_DEV_SERVER: `http://127.0.0.1:${PORT}` },
  });
  astro.on('exit', (code) => process.exit(code ?? 0));
  // Arrêter le serveur arrête aussi astro dev.
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => astro.kill(signal));
});
