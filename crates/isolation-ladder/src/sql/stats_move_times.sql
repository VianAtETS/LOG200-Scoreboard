-- Temps de réflexion de :team par build, dans l'ordre où ils ont joué pour la première fois.
-- Serveur principal seulement : sur un nœud plus lent, les mêmes coups prennent plus de temps.
WITH scope AS (
    SELECT * FROM games
    WHERE ((:session IS NULL AND tournament_id IS NULL) OR session = :session)
      AND json_extract(record, '$.node') IS NULL
),
sides AS (
    SELECT seq, time_ms, red_build AS build, 'red' AS color FROM scope WHERE red_team = :team
    UNION ALL
    SELECT seq, time_ms, black_build, 'black' FROM scope WHERE black_team = :team
),
builds AS (
    SELECT s.build,
           count(*) AS moves,
           CAST(round(avg(m.ms)) AS INTEGER) AS avg_ms,
           max(m.ms) AS max_ms,
           -- Le premier coup de chaque joueur a un délai de grâce (JVM à froid) : hors du compte.
           sum(m.ply >= 2 AND m.ms >= 0.9 * s.time_ms) AS near_limit,
           min(s.seq) AS since
    FROM sides s JOIN moves m ON m.game = s.seq AND m.color = s.color
    WHERE m.ms IS NOT NULL
    GROUP BY s.build
)
SELECT json_group_array(
    json_object('build', build, 'moves', moves, 'avg_ms', avg_ms, 'max_ms', max_ms, 'near_limit', near_limit)
    ORDER BY since
)
FROM builds
