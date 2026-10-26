//! Adversaire de test pour le ladder et outil de conformance : chaque coup adverse et chaque fin de
//! partie annoncée sont vérifiés avec notre moteur.

mod probe;

use std::fmt;
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use rand::SeedableRng;
use rand::rngs::StdRng;

use isolation::board::{Board, Color, Move, SIZE};
use isolation::cpu;
use isolation::protocol::{LineConn, Outcome, RecvError, ServerMsg};
use isolation::rules;
use isolation::strategy::Strategy;

use probe::ProbeKind;

#[derive(Parser)]
#[command(
    name = "isolation-bot",
    version,
    about = "Client de test et de conformance pour Isolation"
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8888)]
    port: u16,
    #[arg(long, value_enum, default_value_t = Strategy::Greedy)]
    strategy: Strategy,
    /// Graine du hasard, pour rejouer une partie à l'identique.
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long, default_value_t = 50)]
    margin_ms: u64,
    /// Envoie d'abord, à chaque tour, un coup que notre moteur refuse.
    #[arg(long)]
    probe_illegal: bool,
    #[arg(long, value_enum)]
    expect: Option<Strategy>,
    #[arg(long, default_value_t = 30_000)]
    connect_timeout_ms: u64,
    /// Affiche les messages échangés.
    #[arg(long)]
    verbose: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(report) => {
            eprint!("{report}");
            if report.is_clean() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: &Cli) -> anyhow::Result<Report> {
    if cli.expect.is_some_and(|s| s != Strategy::Niveau1) {
        bail!("--expect : seul niveau-1 se vérifie coup par coup");
    }
    let rng = cli.seed.map_or_else(rand::make_rng, StdRng::seed_from_u64);
    let conn = connect(&cli.host, cli.port, Duration::from_millis(cli.connect_timeout_ms))?;
    let mut client = Client {
        player: Player {
            conn,
            strategy: cli.strategy,
            rng,
            margin: Duration::from_millis(cli.margin_ms),
            probe: cli.probe_illegal,
            expect: cli.expect.is_some(),
            verbose: cli.verbose,
        },
        report: Report::default(),
        game: None,
    };
    client.run()?;
    Ok(client.report)
}

fn connect(host: &str, port: u16, timeout: Duration) -> anyhow::Result<LineConn> {
    let start = Instant::now();
    loop {
        match TcpStream::connect((host, port)) {
            Ok(stream) => return LineConn::new(stream).context("socket"),
            Err(e) if start.elapsed() > timeout => {
                return Err(e).with_context(|| format!("connexion à {host}:{port}"));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[derive(Default)]
struct Report {
    games: u32,
    checked: u32,
    expected: u32,
    mismatches: Vec<String>,
    probes_rejected: u32,
    probes_accepted: Vec<String>,
}

impl Report {
    fn mismatch(&mut self, what: String) {
        self.mismatches.push(what);
    }

    fn is_clean(&self) -> bool {
        self.mismatches.is_empty() && self.probes_accepted.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "bilan : {} partie(s), {} coups adverses vérifiés",
            self.games, self.checked
        )?;
        if self.expected > 0 {
            write!(f, " ({} comparés au niveau 1)", self.expected)?;
        }
        writeln!(
            f,
            ", {} divergence(s), sondes refusées {} / acceptées {}",
            self.mismatches.len(),
            self.probes_rejected,
            self.probes_accepted.len()
        )?;
        self.mismatches
            .iter()
            .chain(&self.probes_accepted)
            .try_for_each(|m| writeln!(f, "  DIVERGENCE {m}"))
    }
}

/// Notre coup envoyé, en attente de verdict : `4` le refuse, `3` ou `5` l'accepte.
enum Pending {
    Move(Move),
    Probe(Move, ProbeKind),
}

impl Pending {
    const fn mv(&self) -> Move {
        match *self {
            Self::Move(mv) | Self::Probe(mv, _) => mv,
        }
    }
}

struct Game {
    board: Board,
    color: Color,
    pending: Option<Pending>,
    /// Le `5` final peut répéter notre dernier coup : il ne faut pas le rejouer.
    last_own: Option<Move>,
    probed_this_turn: bool,
    turn: usize,
    budget: Duration,
    deadline: Instant,
    search: SearchStats,
    opponent_moves: Vec<(Board, Move)>,
}

#[derive(Default)]
struct SearchStats {
    moves: u32,
    fallbacks: u32,
    deepest: u8,
}

impl Game {
    fn new(color: Color, budget: Duration) -> Self {
        Self {
            board: Board::initial(),
            color,
            pending: None,
            last_own: None,
            probed_this_turn: false,
            turn: 0,
            budget,
            deadline: Instant::now() + budget,
            search: SearchStats::default(),
            opponent_moves: Vec::new(),
        }
    }

    fn new_turn(&mut self) {
        self.turn += 1;
        self.probed_this_turn = false;
        self.deadline = Instant::now() + self.budget;
    }

    /// Le serveur a accepté le coup en attente.
    fn settle(&mut self, report: &mut Report) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let mv = pending.mv();
        if let Pending::Probe(_, kind) = pending {
            report
                .probes_accepted
                .push(format!("sonde {kind} acceptée : {mv}\n{}", self.board));
        }
        self.board.apply(mv);
        self.last_own = Some(mv);
    }

    fn opponent_move(&mut self, mv: Move, report: &mut Report) {
        report.checked += 1;
        if !rules::is_legal(&self.board, self.color.other(), mv) {
            report.mismatch(format!("coup adverse jugé illégal : {mv}\n{}", self.board));
        }
        self.opponent_moves.push((self.board.clone(), mv));
        self.board.apply(mv);
    }

    fn check_level_1(&self, report: &mut Report) {
        let before = report.mismatches.len();
        for (board, mv) in &self.opponent_moves {
            report.expected += 1;
            let accepted = cpu::accepted_moves(board, self.color.other());
            if !accepted.contains(mv) {
                let accepted: Vec<String> = accepted.iter().map(Move::to_string).collect();
                report.mismatch(format!(
                    "coup {mv} hors des coups du niveau 1 ({})\n{board}",
                    accepted.join(" ")
                ));
            }
        }
        eprintln!(
            "niveau 1 : {} coups comparés, {} divergence(s)",
            self.opponent_moves.len(),
            report.mismatches.len() - before
        );
    }

    fn check_end(&self, outcome: Outcome, report: &mut Report) {
        let confirmed = match outcome {
            Outcome::Win => !rules::has_moves(&self.board, self.color.other()),
            Outcome::Loss => !rules::has_moves(&self.board, self.color),
            Outcome::Timeout => true,
        };
        if !confirmed {
            report.mismatch(format!("fin {outcome} non confirmée par notre moteur\n{}", self.board));
        }
    }
}

struct Player {
    conn: LineConn,
    strategy: Strategy,
    rng: StdRng,
    margin: Duration,
    probe: bool,
    expect: bool,
    verbose: bool,
}

impl Player {
    fn play(&mut self, game: &mut Game, report: &mut Report) -> anyhow::Result<()> {
        if self.probe && !game.probed_this_turn {
            game.probed_this_turn = true;
            if let Some((mv, kind)) = probe::pick(&game.board, game.color, game.turn) {
                return self.send(game, Pending::Probe(mv, kind));
            }
        }
        let Some(choice) = self
            .strategy
            .choose(&game.board, game.color, game.deadline, &mut self.rng)
        else {
            report.mismatch(format!("demande de coup sans coup légal\n{}", game.board));
            return Ok(());
        };
        if let Some(depth) = choice.depth {
            game.search.moves += 1;
            game.search.fallbacks += u32::from(depth == 0);
            game.search.deepest = game.search.deepest.max(depth);
        }
        self.send(game, Pending::Move(choice.mv))
    }

    fn send(&mut self, game: &mut Game, pending: Pending) -> anyhow::Result<()> {
        let mv = pending.mv();
        if self.verbose {
            eprintln!("-> {mv}");
        }
        self.conn.send(mv).context("envoi")?;
        game.pending = Some(pending);
        Ok(())
    }
}

struct Client {
    player: Player,
    report: Report,
    game: Option<Game>,
}

impl Client {
    fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let line = match self.player.conn.recv(None) {
                Ok(line) => line,
                Err(RecvError::Closed) => return Ok(()),
                Err(e) => return Err(e).context("lecture"),
            };
            if self.player.verbose {
                eprintln!("<- {}", abbreviate(&line));
            }
            match line.parse() {
                Ok(msg) => self.handle(msg)?,
                Err(e) => self.report.mismatch(format!("message illisible : {e}")),
            }
        }
    }

    fn handle(&mut self, msg: ServerMsg) -> anyhow::Result<()> {
        match msg {
            ServerMsg::NewGame {
                rows,
                cols,
                time_ms,
                color,
            } => {
                if (rows, cols) != (SIZE.into(), SIZE.into()) {
                    bail!("plateau {rows}x{cols} non géré");
                }
                eprintln!("nouvelle partie : {color}, {time_ms} ms");
                let budget = Duration::from_millis(time_ms).saturating_sub(self.player.margin);
                self.game = Some(Game::new(color, budget));
            }
            ServerMsg::Board { color, board } => {
                let game = self.game.as_mut().context("plateau reçu avant le message 0")?;
                game.board = board;
                if color == Color::Red {
                    game.new_turn();
                    self.player.play(game, &mut self.report)?;
                }
            }
            ServerMsg::Request(opponent) => {
                let game = self.game.as_mut().context("demande de coup hors partie")?;
                game.settle(&mut self.report);
                if let Some(mv) = opponent {
                    game.opponent_move(mv, &mut self.report);
                }
                game.new_turn();
                self.player.play(game, &mut self.report)?;
            }
            ServerMsg::Invalid => {
                let game = self.game.as_mut().context("4 hors partie")?;
                match game.pending.take() {
                    Some(Pending::Probe(..)) => self.report.probes_rejected += 1,
                    Some(Pending::Move(mv)) => self.report.mismatch(format!("notre coup {mv} refusé par le serveur")),
                    None => self.report.mismatch("4 reçu sans coup en attente".into()),
                }
                self.player.play(game, &mut self.report)?;
            }
            ServerMsg::End { outcome, last } => {
                let mut game = self.game.take().context("fin de partie hors partie")?;
                game.settle(&mut self.report);
                if let Some(mv) = last
                    && Some(mv) != game.last_own
                {
                    game.opponent_move(mv, &mut self.report);
                }
                game.check_end(outcome, &mut self.report);
                if self.player.expect {
                    game.check_level_1(&mut self.report);
                }
                self.report.games += 1;
                eprintln!("fin : {outcome} ({} cases détruites)", game.board.destroyed());
                let search = &game.search;
                if search.moves > 0 {
                    eprintln!(
                        "recherche : {} coups, profondeur max {}, {} au hasard faute de temps",
                        search.moves, search.deepest, search.fallbacks
                    );
                }
            }
        }
        Ok(())
    }
}

fn abbreviate(line: &str) -> String {
    match line.char_indices().nth(60) {
        Some((cut, _)) => format!("{}...", &line[..cut]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviate_cuts_on_char_boundaries() {
        let line = "é".repeat(70);
        assert_eq!(abbreviate(&line), format!("{}...", "é".repeat(60)));
        assert_eq!(abbreviate("court"), "court");
    }
}
