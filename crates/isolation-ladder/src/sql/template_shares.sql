-- Les parts du gabarit d'un build, tant que son équipe le publie.
SELECT p.template
FROM profiles p
JOIN profiling c ON c.team = p.team AND c.build = p.build AND c.public
WHERE p.team = :team AND p.build = :build AND p.template IS NOT NULL
