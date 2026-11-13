# Déployer le ladder

Le ladder et sa page tournent avec un `docker compose up -d`. Chaque jar joue dans un bac à sable gVisor sans réseau (voir "Isolation des joueurs" dans `docs/formats.md`).

## Prérequis

- Une machine avec Docker : Linux de préférence, Docker Desktop fonctionne aussi.
- Au moins 4 cœurs : chaque partie réserve un cœur par joueur (`cpuset_red`, `cpuset_black` dans `ladder.toml`).
- Pour jouer plusieurs parties en même temps, déclarer un `[[slot]]` par partie dans `ladder.toml`. Il faut compter 2 cœurs et 2 × `memory` de RAM par slot, plus 1 ou 2 cœurs pour Docker et les arbitres. Avec 8 cœurs, par exemple, 3 slots sur les cœurs 2 à 7.
- Le service `ladder` tourne en mode privilégié, parce qu'il héberge le Docker interne et gVisor qui isolent les jars. Le code des équipes, lui, n'a ni privilège ni réseau.

## Installation

1. Télécharger `isolation-scoreboard-deploy.zip` depuis la [dernière release](https://github.com/VianAtETS/LOG200-Scoreboard/releases/latest) et l'extraire.
2. Créer la config : `cp ladder.example.toml ladder.toml`, puis régler au moins la session et le temps par coup.
3. Lancer `docker compose up -d` : le ladder, l'API et les données sur `http://<machine>:8080/` (autre port : `WEB_PORT=80 docker compose up -d`).
4. La page : sur GitHub Pages (section plus bas), ou sur ce serveur avec `COMPOSE_PROFILES=web docker compose up -d`, à `http://<machine>:8080/`.

## Inscrire les équipes

Les équipes soumettent elles-mêmes leur jar (formulaire de la page, `curl` ou CI ; voir "Soumission des jars" dans `docs/formats.md`). Pour chaque équipe :

```sh
docker compose exec ladder isolation-ladder add-team eq07
```

La commande affiche le token de l'équipe, une seule fois : le lui transmettre en privé. Une équipe entre au classement dès que son premier jar a passé le smoke test ; inutile de redémarrer.

Pour que chaque push sur `main` soumette le jar, ajouter [`templates/ladder.yml`](https://github.com/VianAtETS/LOG200-Scoreboard/blob/main/templates/ladder.yml) au template du lab. Chaque équipe règle alors la variable `LADDER_URL` et le secret `LADDER_TOKEN` de son dépôt (voir "Depuis la CI" dans `docs/formats.md`).

## Au quotidien

| Pour… | Faire |
| --- | --- |
| Suivre les parties | `docker compose logs -f ladder` |
| Voir les soumissions | `docker compose exec ladder sqlite3 -readonly state/ladder.db "SELECT ts, team, verdict, detail FROM submissions ORDER BY rowid DESC LIMIT 20"` (verdict et raison de chaque refus) |
| Interroger les parties | `docker compose exec ladder sqlite3 -readonly state/ladder.db`, ou les statistiques de `/api/stats` (voir `docs/api.md`) |
| Retirer une équipe | supprimer sa ligne dans `jars/teams.toml` et son dossier `jars/<équipe>/` |
| Changer le temps par coup | `time_ms` dans `ladder.toml`, puis `docker compose restart ladder` ; les bots `niveau-N` suivent le temps annoncé |
| Mettre à jour | `docker compose pull && docker compose up -d` |
| Mettre à jour la page seule (profil `web`) | `docker compose pull web && docker compose up -d web` : le ladder continue |
| Fixer une version | `SCOREBOARD_TAG=v0.2.0 docker compose up -d` |
| Arrêter | `docker compose down` (les parties restent dans `state/ladder.db`) |
| Arrêter tout de suite | `docker compose down -t 0` : la partie en cours est perdue (rejouée si c'est un match de tournoi) |

Redémarrer, mettre à jour ou arrêter n'interrompt aucune partie : le ladder n'en lance plus, finit celles en cours (quelques minutes, 15 au plus), puis s'arrête. Pendant ce temps, une nouvelle soumission reçoit un 503.
| Sauvegarder | `docker compose exec ladder sqlite3 state/ladder.db ".backup state/sauvegarde.db"`, puis copier `state/sauvegarde.db` hors du serveur. Ne pas copier `ladder.db` pendant que le ladder tourne : la copie peut être incohérente. |

Les parties et le journal des soumissions sont dans `state/ladder.db` (format dans `docs/formats.md`, "Base des parties"). Un serveur installé avant la base doit ajouter `- ./state:/srv/state` aux volumes du service `ladder` de son `compose.yml` avant de mettre à jour : sans ce volume, le ladder refuse de démarrer plutôt que de perdre ses parties. Au premier démarrage après la mise à jour, le ladder y importe `data/games.jsonl` et `jars/submissions.jsonl`, puis les renomme en `.migrated` : les garder comme sauvegarde, ou les supprimer une fois la base vérifiée. Les journaux des joueurs vont dans `data/logs/`, les jars et les tokens dans `jars/`. La page ne sert ni `state/`, ni `data/logs/`, ni `jars/`.

## Tournoi hebdomadaire

La section `[tournament]` de `ladder.toml` tire un tournoi à chaque créneau de `weekly` (`"wed 07:00, sun 19:00"`, heure de Montréal) entre les équipes qui ont un jar accepté : double élimination à 5 s par coup, jars figés au tirage. Le ladder le joue en priorité, puis reprend ses parties ; la page Tournoi montre le tableau. Un créneau manqué de moins de 2 h (ladder arrêté) est rattrapé au démarrage. Règles et fichiers : `docs/spec-tournoi-hebdo.md`.

| Pour… | Faire |
| --- | --- |
| Lancer un tournoi maintenant | `docker compose exec ladder isolation-ladder tournament start /srv/ladder.toml` |
| Débloquer un match suspendu (3 relances en échec) | `docker compose exec ladder isolation-ladder tournament resume /srv/ladder.toml tournoi-2026-11-08 W3` |
| Désactiver | retirer `weekly` de `[tournament]`, puis `docker compose restart ladder` |

La commande dépose une requête dans `jars/admin/`, que le ladder lit en moins de 10 s.

Un jar absent (`no_show`) ne perd pas tout de suite en tournoi : la partie est rejouée 30 s plus tard, et l'absence ne compte qu'au dernier essai (`retries`, 3 par défaut). Un jar figé a passé le smoke test ; s'il ne se connecte pas, c'est le plus souvent la machine qui rame.

### Zombies et surchauffe

Le conteneur du ladder doit avoir un init en PID 1 (l'image lance `docker-init`, et le compose ajoute `init: true`). Chaque bac à sable laisse un processus `containerd-shim` orphelin ; sans init, personne ne le ramasse et il reste zombie. Le 2026-11-08, 37 581 zombies accumulés en 3 jours ont atteint la limite de processus du conteneur : plus aucun joueur ne démarrait, toutes les parties finissaient en `no_show`, et deux slots sont restés bloqués. Au démarrage, le ladder avertit s'il est lui-même le PID 1. Pour vérifier : `ps -eo stat --ppid $(docker inspect -f '{{.State.Pid}}' <conteneur>) | grep -c Z` sur l'hôte doit rester proche de 0.

La section `[thermal]` de `ladder.toml` est un filet de sécurité contre la vraie surchauffe : au-dessus de `pause_c` (90 °C par défaut), aucune nouvelle partie ne part, jusqu'à ce que le CPU redescende sous `resume_c` (80 °C). Réglez `pause_c` au-dessus de la température normale de la machine en charge : un CPU de bureau peut monter à 95–100 °C sans problème, et le garde-fou bloquerait alors le ladder pour rien. `docker compose logs ladder | grep thermal` montre le capteur trouvé au démarrage et chaque pause. Le conteneur est privilégié, il lit donc le `/sys` de l'hôte. Sans capteur (dans une VM, par exemple), le ladder l'annonce au démarrage et joue sans garde-fou.

## Nom de domaine et HTTPS

Hors du périmètre de ce compose : placer le reverse proxy de votre choix (Caddy, Nginx…) devant le port 8080.

Pour un pic de visiteurs (des centaines à la fois), un CDN comme Cloudflare absorbe la charge. Le serveur marque `/data/*` `Cache-Control: public, max-age=15`, le rythme de rafraîchissement de la page. Cloudflare ne met pas en cache les `.json` et `.jsonl` par défaut : ajouter une Cache Rule sur les chemins `/data/*`, `/api/games`, `/api/games/*` et `/api/stats/*`, "Eligible for cache", durée en cache de l'origine respectée. Le serveur sert alors chaque fichier et chaque statistique une fois par 15 s et par point de présence, peu importe l'audience, et chaque partie du replay une seule fois (elle est marquée `immutable`). Sans cette règle, chaque appel à `/api/stats` recalcule la statistique (jusqu'à 0,3 s de CPU sur 50 000 parties).

Attention à `/api/submit` : Cloudflare coupe une requête sans réponse après 100 s (erreur 524), alors qu'un smoke test attend la partie en cours et les soumissions précédentes, ce qui peut prendre plusieurs minutes. Le ladder poursuit quand même : un 524 veut dire "verdict inconnu", pas "refusé" (voir la table `submissions`, plus haut, ou la version affichée sur la page). La page et `templates/ladder.yml` soumettent donc avec `?async` (voir docs/formats.md) : la réponse arrive tout de suite et le verdict s'interroge ensuite, si bien que le proxy ne coupe plus rien. Une CI qui soumet encore sans `?async` doit viser un nom d'hôte non proxifié (nuage gris dans Cloudflare) qui mène au même serveur.

## Direct des parties

La page Direct (`direct.html`) montre chaque partie en cours coup par coup, et le replay suit une partie lancée depuis un badge "direct". Le ladder les diffuse par une WebSocket sur `/api/live`, en lecture seule et sans authentification (protocole dans `docs/formats.md`). La section `[live]` de `ladder.toml` l'ouvre, le `Caddyfile` la relaie déjà ; sans elle, la page affiche "Direct indisponible" et le reste fonctionne.

- Une ancienne `ladder.toml` n'a pas de section `[live]` : ajouter `[live]` puis `listen = "0.0.0.0:8092"`, et `docker compose restart ladder`.
- Avec Nginx Proxy Manager, cocher "Websockets Support", comme pour `/api/remote`.
- Le ladder accepte 512 spectateurs à la fois et ferme toute connexion au bout de 2 h (la page se reconnecte seule). Derrière Cloudflare, une règle de rate limiting sur le chemin `/api/live` (par exemple 20 requêtes par minute et par IP) empêche un seul client de prendre toutes les places ; le ladder ne voit que l'adresse du proxy.

## Équipes distantes (optionnel)

Une équipe peut jouer sans téléverser son jar : il tourne sur sa machine, et `isolation-remote` relaie ses coups par une WebSocket sur `/api/remote` (voir "Jouer à distance" dans `docs/formats.md`). Pour l'ouvrir, décommenter la section `[remote]` de `ladder.toml`, puis `docker compose restart ladder`. Le `Caddyfile` relaie déjà `/api/remote` ; rien n'est exposé de plus que le port 8080.

L'équipe utilise son token habituel et l'adresse de la page : `isolation-remote --url https://ladder.example.com -- java -jar Client-eq07.jar`. Ses parties portent le badge "distant", et la case "Vérifiées seulement" de la page les retire du classement.

- Le mode distant demande HTTPS (section précédente) : le token passe dans l'en-tête de la WebSocket. `isolation-remote` avertit l'équipe si l'adresse est en `http://`.
- Avec Nginx Proxy Manager, cocher "Websockets Support" sur l'hôte proxy du ladder : sans elle, l'upgrade de `/api/remote` n'est pas relayé et le client voit une erreur 400 ou 502.
- Cloudflare relaie les WebSockets, mais coupe une connexion inactive après 100 s. Le ladder envoie un ping toutes les 10 s : la connexion reste ouverte.
- La latence compte dans le temps par coup de l'équipe. Comme pour `LADDER_URL`, un nom d'hôte non proxifié évite le détour par Cloudflare.
- `docker compose logs ladder | grep distant` montre les connexions. Le diagnostic du relais de chaque partie est dans `data/logs/`.

## Nœuds de calcul (optionnel)

D'autres machines du LAN peuvent jouer des parties pour le ladder : un nœud prend les paires portables (bots `portable = true`, équipes dont le build actif répond en moins de 100 ms), qui comptent exactement comme les autres. Détails et protocole : "Nœuds" dans `docs/formats.md`.

1. Sur le ladder, décommenter `[nodes]` dans `ladder.toml`, puis inscrire le nœud : `docker compose exec ladder isolation-ladder add-node node-a --nodes jars/nodes.toml`. Le token s'affiche une fois ; seul son sha256 est gardé.
2. Publier `[nodes].listen` (8093 par défaut) **sur l'adresse LAN seulement** et limiter la source aux nœuds dans le pare-feu. Ce port ne passe jamais par Caddy ni par un proxy public.
3. Sur le nœud, la même image que le ladder, en conteneur privilégié (Docker interne + gVisor), avec la commande `node`, `node.toml` (voir `node.example.toml`) et le token dans `token_file`.
4. `docker compose restart ladder`. Le nœud joue d'abord une partie bot contre bot (la sonde), puis apporte ses slots ; `data/nodes.json` montre les nœuds connectés, prêts ou en quarantaine.

- Le nœud et le ladder doivent tourner la même version : un nœud d'une autre version est refusé jusqu'à sa mise à jour.
- Un nœud qui tombe en pleine partie ne coûte rien à personne : la partie est annulée puis rejouée ailleurs. Trois échecs d'affilée le mettent en quarantaine 10 min.
- Le nœud n'exécute aucune commande venue du ladder : seulement les bots de sa liste et des jars identifiés par leur sha256. Un jar qui sortirait de gVisor serait root dans le conteneur privilégié du nœud, comme au ladder.

## Page sur GitHub Pages


Au lieu du profil `web`, la page peut être publiée sur GitHub Pages : une modification de `web/` est alors en ligne à chaque push sur `main`, sans tag. Elle lit les parties et l'API sur ce serveur, qui doit donc être en HTTPS (section "Nom de domaine et HTTPS").

1. Dans le dépôt GitHub, Settings > Pages : source "GitHub Actions".
2. Settings > Secrets and variables > Actions > Variables : créer `SCOREBOARD_URL` avec l'adresse du serveur, par exemple `https://ladder.example.com` (sans `/` final).
3. Lancer le workflow `Pages` (onglet Actions, "Run workflow") ou pousser une modification de `web/`.

Le serveur accepte déjà les requêtes venant d'un autre site (en-têtes CORS du `Caddyfile`) : rien à changer de ce côté, hormis être à jour.

## Tester la page en local

L'image de la page tourne seule et lit n'importe quel serveur en HTTPS :

```sh
docker run --rm -p 8000:80 -e SCOREBOARD_SERVER=https://ladder.example.com ghcr.io/vianatets/log200-scoreboard-web
```

Puis ouvrir `http://localhost:8000/web/`.
