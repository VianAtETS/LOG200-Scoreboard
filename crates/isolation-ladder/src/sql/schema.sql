-- Schéma de la base du ladder (version dans PRAGMA user_version, voir db.rs).
CREATE TABLE IF NOT EXISTS games (
    seq INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    session TEXT NOT NULL,
    ts TEXT NOT NULL,
    time_ms INTEGER NOT NULL,
    red_team TEXT NOT NULL,
    red_build TEXT,
    red_remote INTEGER NOT NULL,
    black_team TEXT NOT NULL,
    black_build TEXT,
    black_remote INTEGER NOT NULL,
    winner TEXT NOT NULL,
    reason TEXT NOT NULL,
    invalid_red INTEGER NOT NULL,
    invalid_black INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    plies INTEGER NOT NULL,
    opening TEXT NOT NULL,
    tournament_id TEXT,
    tournament_match TEXT,
    tournament_game INTEGER,
    record TEXT NOT NULL,
    -- seq de la première partie de la paire du ladder ; NULL pour un tournoi (voir pair_game.sql).
    pair INTEGER
);
CREATE INDEX IF NOT EXISTS games_session ON games(session);
CREATE INDEX IF NOT EXISTS games_pair ON games(pair);
CREATE INDEX IF NOT EXISTS games_red ON games(red_team);
CREATE INDEX IF NOT EXISTS games_black ON games(black_team);
CREATE TABLE IF NOT EXISTS moves (
    game INTEGER NOT NULL REFERENCES games(seq),
    ply INTEGER NOT NULL,
    color TEXT NOT NULL,
    move TEXT NOT NULL,
    ms INTEGER,
    PRIMARY KEY (game, ply)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS ratings (
    game_id TEXT NOT NULL,
    team TEXT NOT NULL,
    all_rating REAL NOT NULL,
    all_delta REAL NOT NULL,
    session_rating REAL NOT NULL,
    session_delta REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS ratings_team ON ratings(team);
CREATE TABLE IF NOT EXISTS submissions (
    ts TEXT NOT NULL,
    team TEXT NOT NULL,
    hash TEXT NOT NULL,
    verdict TEXT NOT NULL,
    detail TEXT NOT NULL
);
-- Profil d'un jar dans une partie, privé à son équipe (docs/spec-analyse-isola.md).
CREATE TABLE IF NOT EXISTS profiles (
    game INTEGER NOT NULL REFERENCES games(seq),
    color TEXT NOT NULL,
    team TEXT NOT NULL,
    build TEXT NOT NULL,
    turns INTEGER NOT NULL,
    moves INTEGER NOT NULL,
    overhead_ms INTEGER NOT NULL,
    tree TEXT NOT NULL,
    arena TEXT NOT NULL,
    template TEXT,
    PRIMARY KEY (game, color)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS profiles_team ON profiles(team);
-- Choix d'une équipe pour un build : public implique private.
CREATE TABLE IF NOT EXISTS profiling (
    team TEXT NOT NULL,
    build TEXT NOT NULL,
    private INTEGER NOT NULL,
    public INTEGER NOT NULL CHECK (private OR NOT public),
    PRIMARY KEY (team, build)
) WITHOUT ROWID;
