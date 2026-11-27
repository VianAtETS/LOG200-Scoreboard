-- Victoires de chaque paire d'équipes, `a` avant `b` dans l'ordre alphabétique. Les nulles ne
-- comptent pour personne : `games - a_wins - b_wins`.
WITH scope AS (
    SELECT * FROM games
    WHERE (:session IS NULL AND tournament_id IS NULL) OR session = :session
),
pairs AS (
    SELECT min(red_team, black_team) AS a,
           max(red_team, black_team) AS b,
           count(*) AS games,
           coalesce(sum(CASE winner WHEN 'red' THEN red_team WHEN 'black' THEN black_team END = min(red_team, black_team)), 0) AS a_wins,
           coalesce(sum(CASE winner WHEN 'red' THEN red_team WHEN 'black' THEN black_team END = max(red_team, black_team)), 0) AS b_wins
    FROM scope
    WHERE red_team <> black_team
    GROUP BY a, b
)
SELECT json_group_array(
    json_object('a', a, 'b', b, 'games', games, 'a_wins', a_wins, 'b_wins', b_wins)
    ORDER BY a, b
)
FROM pairs
