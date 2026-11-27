-- Remplace le choix de l'équipe pour ce build ; public sans private est refusé par le schéma.
INSERT INTO profiling (team, build, private, public) VALUES (:team, :build, :private, :public)
ON CONFLICT (team, build) DO UPDATE SET private = excluded.private, public = excluded.public
