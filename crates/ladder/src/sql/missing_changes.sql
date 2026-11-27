-- Parties sans effets après la partie :after, dans l'ordre de jeu : à faire rejouer par `check`.
SELECT seq, record FROM games WHERE changes IS NULL AND seq > :after ORDER BY seq LIMIT :limit
