-- Bilan de chaque build (jar soumis) de :team, dans l'ordre où ils ont joué pour la première fois.
WITH scope AS (
    SELECT * FROM games
    WHERE (:session IS NULL AND tournament_id IS NULL) OR session = :session
),
sides AS (
    SELECT seq, ts, red_build AS build, winner = 'red' AS won FROM scope WHERE red_team = :team
    UNION ALL
    SELECT seq, ts, black_build, winner = 'black' FROM scope WHERE black_team = :team
),
builds AS (
    SELECT build, count(*) AS games, sum(won) AS wins, min(seq) AS since, min(ts) AS first, max(ts) AS last
    FROM sides
    WHERE build IS NOT NULL
    GROUP BY build
)
SELECT json_group_array(
    json_object('build', build, 'games', games, 'wins', wins, 'first', first, 'last', last)
    ORDER BY since
)
FROM builds
