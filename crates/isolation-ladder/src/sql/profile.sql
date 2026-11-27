-- Le profil d'une équipe dans une partie, en JSON (openapi::GameProfile) ; aucune ligne si elle n'y
-- a pas de profil.
SELECT json_object(
    'game', g.id,
    'color', p.color,
    'build', p.build,
    'turns', p.turns,
    'moves', p.moves,
    'overhead_ms', p.overhead_ms,
    'tree', json(p.tree),
    'arena', json(p.arena)
)
FROM profiles p JOIN games g ON g.seq = p.game
WHERE g.id = :id AND p.team = :team
