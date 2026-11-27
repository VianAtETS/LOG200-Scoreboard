-- Rating de :team après chacune de ses parties classées, dans l'ordre où elles ont été jouées.
SELECT json_group_array(
    json_object(
        'game', r.game_id, 'ts', g.ts, 'session', g.session,
        'all', json_object('rating', r.all_rating, 'delta', r.all_delta),
        'in_session', json_object('rating', r.session_rating, 'delta', r.session_delta)
    ) ORDER BY g.seq
)
FROM ratings r JOIN games g ON g.id = r.game_id
WHERE r.team = :team AND (:session IS NULL OR g.session = :session)
