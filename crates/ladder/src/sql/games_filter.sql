-- Les :limit dernières parties qui passent les filtres (chacun ignoré s'il est NULL), avant la
-- partie :before exclue, de la plus ancienne à la plus récente.
SELECT record FROM (
    SELECT seq, record FROM games
    WHERE (:team IS NULL OR :team IN (red_team, black_team))
      AND (:opponent IS NULL OR :opponent IN (red_team, black_team))
      AND (:session IS NULL OR session = :session)
      AND (:before IS NULL OR seq < (SELECT seq FROM games WHERE id = :before))
    ORDER BY seq DESC LIMIT :limit
) ORDER BY seq
