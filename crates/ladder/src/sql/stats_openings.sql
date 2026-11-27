-- Ouvertures imposées et premiers coups clients les plus joués, avec leurs victoires.
-- shortcut: 50 lignes par liste, sans pagination ; ajouter :limit si la page en veut plus.
WITH scope AS (
    SELECT * FROM games
    WHERE (:session IS NULL AND tournament_id IS NULL) OR session = :session
),
openings AS (
    SELECT opening, count(*) AS games, sum(winner = 'red') AS red_wins
    FROM scope
    GROUP BY opening
    ORDER BY games DESC, opening
    LIMIT 50
),
first_moves AS (
    SELECT m.color, m.move, count(*) AS games, sum(s.winner = m.color) AS wins
    FROM moves AS m JOIN scope AS s ON s.seq = m.game
    WHERE m.ply < 2
    GROUP BY m.color, m.move
),
ranked AS (
    SELECT *, row_number() OVER (PARTITION BY color ORDER BY games DESC, move) AS rank
    FROM first_moves
)
SELECT json_object(
    'openings', json((
        SELECT json_group_array(
            json_object('opening', json(opening), 'games', games, 'red_wins', red_wins)
            ORDER BY games DESC, opening
        )
        FROM openings
    )),
    'first_moves', json_object(
        'red', json((
            SELECT json_group_array(json_object('move', move, 'games', games, 'wins', wins) ORDER BY rank)
            FROM ranked WHERE color = 'red' AND rank <= 50
        )),
        'black', json((
            SELECT json_group_array(json_object('move', move, 'games', games, 'wins', wins) ORDER BY rank)
            FROM ranked WHERE color = 'black' AND rank <= 50
        ))
    )
)
