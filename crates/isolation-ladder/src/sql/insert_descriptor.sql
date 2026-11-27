-- Jamais remplacé : les parties d'une version gardent le descripteur sous lequel elles ont été jouées.
INSERT OR IGNORE INTO descriptors (game, version, descriptor) VALUES (:game, :version, :descriptor)
