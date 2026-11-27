-- Parties du ladder sans paire, dans l'ordre de jeu (voir pair_history dans db.rs).
SELECT seq, red_team, black_team, opening FROM games WHERE pair IS NULL AND tournament_id IS NULL ORDER BY seq
