-- Soumissions de :team, de la plus ancienne à la plus récente. Le détail d'une erreur du ladder
-- reste privé : il peut contenir des chemins du serveur.
SELECT json_group_array(
    json_object('ts', ts, 'hash', hash, 'verdict', verdict,
                'detail', CASE WHEN verdict = 'rejected' THEN detail ELSE '' END)
    ORDER BY rowid
)
FROM submissions
WHERE team = :team
