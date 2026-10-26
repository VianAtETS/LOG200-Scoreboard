# Déployer le ladder

Le ladder et sa page tournent avec un `docker compose up -d`. Chaque jar joue dans un bac à sable gVisor sans réseau (voir « Isolation des joueurs » dans `docs/formats.md`).

## Prérequis

- Une machine avec Docker : Linux de préférence, Docker Desktop fonctionne aussi.
- Au moins 4 cœurs : chaque partie réserve un cœur par joueur (`cpuset_red`, `cpuset_black` dans `ladder.toml`).
- Le service `ladder` tourne en mode privilégié, parce qu'il héberge le Docker interne et gVisor qui isolent les jars. Le code des équipes, lui, n'a ni privilège ni réseau.

## Installation

1. Télécharger `isolation-scoreboard-deploy.zip` depuis la [dernière release](https://github.com/VianAtETS/LOG200-Scoreboard/releases/latest) et l'extraire.
2. Créer la config : `cp ladder.example.toml ladder.toml`, puis régler au moins la session et le temps par coup.
3. Lancer `docker compose up -d`. La page est sur `http://<machine>:8080/` (autre port : `WEB_PORT=80 docker compose up -d`).

## Inscrire les équipes

Les équipes soumettent elles-mêmes leur jar (formulaire de la page, `curl` ou CI ; voir « Soumission des jars » dans `docs/formats.md`). Pour chaque équipe :

```sh
docker compose exec ladder isolation-ladder add-team eq07
```

La commande affiche le token de l'équipe, une seule fois : le lui transmettre en privé. Une équipe entre au classement dès que son premier jar a passé le smoke test ; inutile de redémarrer.

Pour que chaque push sur `main` soumette le jar, ajouter [`templates/ladder.yml`](https://github.com/VianAtETS/LOG200-Scoreboard/blob/main/templates/ladder.yml) au template du lab. Chaque équipe règle alors la variable `LADDER_URL` et le secret `LADDER_TOKEN` de son dépôt (voir « Depuis la CI » dans `docs/formats.md`).

## Au quotidien

| Pour… | Faire |
| --- | --- |
| Suivre les parties | `docker compose logs -f ladder` |
| Voir les soumissions | `jars/submissions.jsonl` (verdict et raison de chaque refus) |
| Retirer une équipe | supprimer sa ligne dans `jars/teams.toml` et son dossier `jars/<équipe>/` |
| Changer le temps par coup | `time_ms` dans `ladder.toml`, puis `docker compose restart ladder` ; les bots `niveau-N` suivent le temps annoncé |
| Mettre à jour | `docker compose pull && docker compose up -d` |
| Fixer une version | `SCOREBOARD_TAG=v0.2.0 docker compose up -d` |
| Arrêter | `docker compose down` (les parties restent dans `data/`) |
| Sauvegarder | copier `data/games.jsonl` |

Les journaux des joueurs vont dans `data/logs/`, les jars et les tokens dans `jars/`. La page ne sert ni l'un ni l'autre.

## Nom de domaine et HTTPS

Hors du périmètre de ce compose : placer le reverse proxy de votre choix (Caddy, Nginx…) devant le port 8080.

Pour un pic de visiteurs (des centaines à la fois), un CDN comme Cloudflare absorbe la charge. Le serveur marque `/data/*` `Cache-Control: public, max-age=15`, le rythme de rafraîchissement de la page. Cloudflare ne met pas en cache les `.json` et `.jsonl` par défaut : ajouter une Cache Rule sur le chemin `/data/*`, « Eligible for cache », durée en cache de l'origine respectée. Le serveur sert alors chaque fichier une fois par 15 s et par point de présence, peu importe l'audience.

Attention à `/api/submit` : Cloudflare coupe une requête sans réponse après 100 s (erreur 524), alors qu'un smoke test attend la partie en cours et les soumissions précédentes, ce qui peut prendre plusieurs minutes. Le ladder poursuit quand même : un 524 veut dire « verdict inconnu », pas « refusé » (voir `jars/submissions.jsonl` ou la version affichée sur la page). Pour que la CI reçoive toujours le verdict, faire pointer son `LADDER_URL` vers un nom d'hôte non proxifié (nuage gris dans Cloudflare) qui mène au même serveur.

## Page sur GitHub Pages (optionnel)

La page reste servie par ce compose, mais elle peut aussi être publiée sur GitHub Pages : une modification de `web/` est alors en ligne à chaque push sur `main`, sans tag ni redémarrage du ladder. Elle lit toujours les parties et l'API sur ce serveur, qui doit donc être en HTTPS (section précédente).

1. Dans le dépôt GitHub, Settings > Pages : source « GitHub Actions ».
2. Settings > Secrets and variables > Actions > Variables : créer `SCOREBOARD_URL` avec l'adresse du serveur, par exemple `https://ladder.example.com` (sans `/` final).
3. Lancer le workflow `Pages` (onglet Actions, « Run workflow ») ou pousser une modification de `web/`.

Le serveur accepte déjà les requêtes venant d'un autre site (en-têtes CORS du `Caddyfile`) : rien à changer de ce côté, hormis être à jour.
