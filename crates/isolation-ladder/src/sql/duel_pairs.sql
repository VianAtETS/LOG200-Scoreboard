-- Paires complètes du ladder entre :a et :b, la plus récente d'abord. :build_a et :build_b, s'ils
-- sont donnés, sont exigés dans les deux parties de la paire.
WITH duel AS (
    SELECT seq, id, pair, opening,
           iif(winner = 'red', red_team, black_team) AS won,
           iif(red_team = :a, red_build, black_build) AS build_a,
           iif(red_team = :a, black_build, red_build) AS build_b
    FROM games
    WHERE pair IS NOT NULL
      AND ((red_team = :a AND black_team = :b) OR (red_team = :b AND black_team = :a))
),
pairs AS (
    SELECT pair,
           json_group_array(id ORDER BY seq) AS games,
           json_group_array(won ORDER BY seq) AS winners,
           min(opening) AS opening
    FROM duel
    WHERE (:build_a IS NULL OR build_a = :build_a) AND (:build_b IS NULL OR build_b = :build_b)
    GROUP BY pair
    HAVING count(*) = 2
)
SELECT json_group_array(
    json_object('games', json(games), 'winners', json(winners), 'opening', json(opening))
    ORDER BY pair DESC
)
FROM pairs
