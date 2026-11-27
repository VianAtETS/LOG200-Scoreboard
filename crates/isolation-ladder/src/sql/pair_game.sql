-- Une partie du ladder rejoint la première de sa paire (:after, son id) ; sans elle, elle est seule.
UPDATE games SET pair = coalesce((SELECT seq FROM games WHERE id = :after), seq)
WHERE seq = :seq AND tournament_id IS NULL
