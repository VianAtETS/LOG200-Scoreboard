-- v2 vers v3, avant schema.sql (qui crée la table des descripteurs).
ALTER TABLE games ADD COLUMN changes TEXT;
