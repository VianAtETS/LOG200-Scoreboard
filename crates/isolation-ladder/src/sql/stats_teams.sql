-- Bilan par équipe. Sans :session, toutes les sessions hors tournois, comme le classement. Une
-- nulle (`winner = 'draw'`) n'est ni une victoire ni une défaite.
WITH scope AS (
    SELECT * FROM games
    WHERE (:session IS NULL AND tournament_id IS NULL) OR session = :session
),
sides AS (
    SELECT red_team AS team, 'red' AS color, winner = 'red' AS won, winner = 'black' AS lost, reason FROM scope
    UNION ALL
    SELECT black_team, 'black', winner = 'black', winner = 'red', reason FROM scope
),
records AS (
    SELECT team,
           count(*) AS games,
           sum(won) AS wins,
           sum(color = 'red') AS red_games,
           sum(color = 'red' AND won) AS red_wins,
           sum(color = 'black') AS black_games,
           sum(color = 'black' AND won) AS black_wins
    FROM sides
    GROUP BY team
),
losses AS (
    SELECT team, json_group_object(reason, n) AS by_reason
    FROM (SELECT team, reason, count(*) AS n FROM sides WHERE lost GROUP BY team, reason)
    GROUP BY team
)
SELECT json_group_array(
    json_object(
        'team', team,
        'games', games,
        'wins', wins,
        'red', json_object('games', red_games, 'wins', red_wins),
        'black', json_object('games', black_games, 'wins', black_wins),
        'losses', json(coalesce(by_reason, '{}'))
    ) ORDER BY wins DESC, games DESC, team
)
FROM records LEFT JOIN losses USING (team)
