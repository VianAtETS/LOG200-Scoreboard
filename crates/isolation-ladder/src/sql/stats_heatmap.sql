-- Cartes de chaleur de :team par build, portées de l'arène isola (arena/web/app.js) : par case,
-- combien de fois son pion y est arrivé et combien de fois elle l'a détruite. Coups clients seulement
-- (pas l'ouverture tirée par le ladder), dans l'ordre où les builds ont joué pour la première fois.
WITH scope AS (
    SELECT * FROM games
    WHERE (:session IS NULL AND tournament_id IS NULL) OR session = :session
),
sides AS (
    SELECT seq, red_build AS build, 'red' AS color FROM scope WHERE red_team = :team
    UNION ALL
    SELECT seq, black_build, 'black' FROM scope WHERE black_team = :team
),
-- Coup canonique « départ-arrivée-détruite » (G1-E3-C12) : `rest` vaut « E3-C12 ».
played AS (
    SELECT s.build, s.seq, substr(m.move, instr(m.move, '-') + 1) AS rest
    FROM sides AS s JOIN moves AS m ON m.game = s.seq AND m.color = s.color
),
squares AS (
    SELECT build, 'moves' AS kind, substr(rest, 1, instr(rest, '-') - 1) AS square FROM played
    UNION ALL
    SELECT build, 'blocks', substr(rest, instr(rest, '-') + 1) FROM played
),
maps AS (
    SELECT build, kind, json_group_object(square, n) AS map
    FROM (SELECT build, kind, square, count(*) AS n FROM squares GROUP BY build, kind, square)
    GROUP BY build, kind
),
builds AS (
    SELECT build, count(DISTINCT seq) AS games, min(seq) AS since FROM played GROUP BY build
)
SELECT json_group_array(
    json_object(
        'build', b.build,
        'games', b.games,
        -- IS : les bots du ladder ont un build NULL.
        'moves', json((SELECT map FROM maps WHERE build IS b.build AND kind = 'moves')),
        'blocks', json((SELECT map FROM maps WHERE build IS b.build AND kind = 'blocks'))
    )
    ORDER BY b.since
)
FROM builds AS b
