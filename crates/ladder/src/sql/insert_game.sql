INSERT INTO games (
    id, session, ts, time_ms,
    red_team, red_build, red_remote,
    black_team, black_build, black_remote,
    winner, reason, invalid_red, invalid_black, duration_ms, plies, opening,
    tournament_id, tournament_match, tournament_game,
    record
) VALUES (
    :id, :session, :ts, :time_ms,
    :red_team, :red_build, :red_remote,
    :black_team, :black_build, :black_remote,
    :winner, :reason, :invalid_red, :invalid_black, :duration_ms, :plies, :opening,
    :tournament_id, :tournament_match, :tournament_game,
    :record
)
