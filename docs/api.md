# API HTTP du ladder

Toutes les routes sont sous `/api/`. Dans le déploiement Docker, Caddy les relaie vers le ladder (voir [deploy/Caddyfile](../deploy/Caddyfile)) et ajoute `Access-Control-Allow-Origin: *` : une page servie ailleurs (GitHub Pages) peut les appeler. Le ladder sert les mêmes routes sur chaque adresse de `ladder.toml` (`listen` de `[submit]`, `[live]` et `[remote]`). Il faut donc au moins une de ces sections pour avoir l'API.

| Route | Accès | Rôle | Détail |
| --- | --- | --- | --- |
| `GET /api/health` | public | `ok`, pour la surveillance | |
| `GET /api/version` | public | version du ladder | |
| `PUT /api/submit` (ou `POST`) | token d'équipe | soumission d'un jar | [formats.md, Soumission des jars](formats.md#soumission-des-jars) |
| `GET /api/submit/{id}` | token d'équipe | verdict d'une soumission `?async` | [formats.md, Soumission des jars](formats.md#soumission-des-jars) |
| `GET`, `PUT /api/profiling` | token d'équipe | profilage du jar actif de l'équipe | ci-dessous |
| `GET /api/games/{id}/profile` | token d'équipe | le profil de son jar dans une partie | ci-dessous |
| `DELETE /api/profiles` | token d'équipe | efface tous ses profils | ci-dessous |
| `GET /api/live` | public | parties en cours, en WebSocket | [formats.md, Direct](formats.md#direct-apilive) |
| `GET /api/remote` | token d'équipe | relais d'une équipe distante, en WebSocket | [formats.md, Jouer à distance](formats.md#jouer-à-distance) |
| `GET /api/games` | public | les parties complètes, en JSONL, filtrables et paginées | ci-dessous |
| `GET /api/games/{id}` | public | une partie complète, pour le replay | ci-dessous |
| `GET /api/stats/{nom}` | public | statistiques calculées dans la base | ci-dessous |
| `GET /api/openapi.json` | public | spec OpenAPI de toutes ces routes HTTP, généré depuis le code | ci-dessous |

Une route inconnue répond 404. Si la base est illisible, les routes de lecture répondent 500 avec `base indisponible`, et le ladder écrit la cause sur sa sortie d'erreur.

## Documentation interactive : `GET /api/openapi.json`

Le ladder génère au démarrage un spec OpenAPI 3.1 de ses routes HTTP (paramètres, codes de retour, schémas des réponses), à partir des annotations `#[utoipa::path]` de `crates/isolation-ladder/src/api.rs` et des types de `crates/isolation-ladder/src/openapi.rs`. Les tests de `db.rs` vérifient que chaque statistique SQL correspond à son schéma : le spec ne peut pas dériver du code sans casser la CI.

La page `api.html` du site (lien « API » dans la barre de navigation, GitHub Pages compris) l'affiche avec Swagger UI, chargé depuis jsDelivr, et permet d'essayer les routes contre le serveur (`GET`, et `PUT /api/submit` avec le token d'équipe ; `POST` n'est pas permis par le préflight CORS de Caddy). Elle suit toujours la version déployée du ladder.

## Parties : `GET /api/games`

Les parties, une par ligne, de la plus ancienne à la plus récente, au même format que `GET /api/games/{id}`, sans `changes` (l'ancien `games.jsonl`). Sans paramètre, les 5000 dernières : le ladder les garde en mémoire et ne relit la base qu'une fois par minute au plus, peu importe le nombre d'appels. Si la base est illisible, il sert sa dernière copie.

Paramètres, tous facultatifs (avec l'un d'eux, le ladder lit la base à chaque appel) :

- `team` : les parties de cette équipe, en rouge ou en noir.
- `opponent` : les parties contre cette équipe. Avec `team`, leurs face-à-face.
- `session` : une session ou un tournoi. Contrairement à `/api/stats`, les tournois ne sont pas exclus par défaut.
- `before` : l'id d'une partie ; seules les parties jouées avant elle (elle exclue). Un id inconnu donne une réponse vide.
- `limit` : le nombre de parties, 5000 au plus (et par défaut).

Pour tout l'historique d'une équipe, remonter page par page en passant comme `before` l'id de la première ligne reçue, jusqu'à une réponse vide :

```sh
curl https://<ladder>/api/games > recent.jsonl
curl "https://<ladder>/api/games?team=eq100&limit=1000"
curl "https://<ladder>/api/games?team=eq100&limit=1000&before=20261111T161123.521Z-eq100-eq214"
```

| Code | Sens |
| --- | --- |
| 200 | `application/x-ndjson`, avec `Cache-Control: public, max-age=60` sans paramètre, `max-age=15` avec. Vide si aucune partie ne correspond. |
| 400 | Paramètre illisible (`limit` non numérique). |
| 500 | `base indisponible` (sans paramètre : seulement si le ladder n'a encore aucune copie en mémoire). |

## Une partie : `GET /api/games/{id}`

La partie telle qu'elle a été enregistrée : l'objet résultat de l'arbitre, complété par le ladder (format dans [formats.md, Base des parties](formats.md#base-des-parties-stateladderdb)), suivi de `changes` : les effets de chaque demi-coup, ouverture comprise (`changes[i]` va avec le i-ème coup de `opening` puis `moves`, chacun une liste de `{"cell", "code", "role"}`). Appliqués dans l'ordre à `initial` du descripteur (`data/game.json`), ils redonnent chaque position sans connaître les règles. `changes` manque le temps que le ladder rejoue une partie enregistrée sans (au démarrage, après la migration) ; la réponse n'est alors pas mise en cache. L'`id` vient de `summary.json`, `teams/<équipe>.jsonl` ou `live.json`.

```sh
curl https://<ladder>/api/games/20261024T142103.512Z-eq07-eq12
```

| Code | Sens |
| --- | --- |
| 200 | `application/json`, avec `Cache-Control: public, max-age=31536000, immutable` : une partie finie ne change plus, un CDN peut la garder. |
| 404 | Aucune partie avec cet id (encore en cours, annulée, ou id inconnu). |

## Profilage : `GET`, `PUT /api/profiling`

Le choix de l'équipe du token pour son jar actif (`current.jar`), désactivé par défaut et pour chaque nouveau build. `PUT` prend `{"private": true, "public": false}` : `private` fait tourner le jar avec le profileur (`sandbox/profiler/`, voir [spec-analyse-isola.md](spec-analyse-isola.md#profileur)) ; `public` publiera en plus la part du gabarit, et demande `private`. Les deux routes répondent l'état du build :

```json
{ "build": "3f9c2a1b5d7e9f01", "private": true, "public": false }
```

| Code | Sens |
| --- | --- |
| 200 | L'état du build actif. |
| 400 | Corps illisible, ou `public` sans `private`. |
| 401 | Token absent ou inconnu. |
| 404 | Aucun jar actif (équipe distante, ou rien soumis). |

Toutes les réponses portent `Cache-Control: private, no-store` et `Vary: Authorization`. Un jar profilé ne joue qu'au serveur principal, dans les parties du ladder : son build n'est plus portable, et ses manches de tournoi se jouent sans profileur. L'arbitre lui accorde une marge de 5 % du temps par coup (25 à 500 ms), qu'il ne voit pas : il reçoit toujours `time_ms`.

## Profils : `GET /api/games/{id}/profile`, `DELETE /api/profiles`

À la fin d'une partie, le profil de chaque jar profilé est écrit avec elle ; il ne figure jamais dans `GET /api/games/{id}`. `GET /api/games/{id}/profile` rend celui de l'équipe du token, son côté seulement :

```json
{
  "game": "20261024T142103.512Z-eq07-eq12", "color": "red", "build": "3f9c2a1b5d7e9f01",
  "turns": 31, "moves": 32, "overhead_ms": 41,
  "tree": {
    "samples": 2480, "sampled_ms": 24790, "think_ms": 25310, "doubtful": false, "rec": 9,
    "methods": { "Client.main": [0, 2480], "Ia.search": [1900, 2400] },
    "paths": { "Client.main>Ia.search": 2400 }
  },
  "arena": [{ "depth": 6, "nodes": 123456 }]
}
```

`tree` est décrit dans [formats.md](formats.md#base-des-parties-stateladderdb) (table `profiles`). `doubtful` : le profileur a vu moins de la moitié du temps de réflexion du jar (thread lecteur dédié, threads virtuels). `arena` : les lignes `ARENA` que le jar imprime lui-même sur stderr.

| Code | Sens |
| --- | --- |
| 200 | Le profil. |
| 401 | Token absent ou inconnu. |
| 404 | Pas de profil de l'équipe dans cette partie : partie inconnue, jouée par d'autres, ou jar non profilé. La réponse est la même dans les trois cas. |

`DELETE /api/profiles` efface tous les profils de l'équipe du token, part publiée comprise, et rend leur nombre : `{"deleted": 12}`. Sans cela, ils restent dans la base. Les deux routes portent `Cache-Control: private, no-store` et `Vary: Authorization`, refus compris.

## Statistiques : `GET /api/stats/{nom}`

Chaque statistique est une requête SQL de `crates/isolation-ladder/src/sql/stats_*.sql`, calculée à chaque appel. Les réponses portent `Cache-Control: public, max-age=15`, le rythme de rafraîchissement de la page. Derrière un CDN, chaque statistique n'est donc recalculée qu'une fois toutes les 15 s (voir la Cache Rule de [deploy/README.md](../deploy/README.md#nom-de-domaine-et-https), à étendre à `/api/stats/*`).

Paramètre commun, facultatif :

- `session` : se limiter à une session (`?session=automne-2026`) ou à un tournoi, qui a sa propre session (`?session=tournoi-2026-11-08`). Sans lui, toutes les sessions sont comptées, mais sans les parties de tournoi, comme dans le classement « toutes sessions ».

Une session inconnue donne une liste vide, pas une erreur.

### `teams` : bilan par équipe

Classé par victoires, puis par nombre de parties. `losses` compte les défaites par raison (`no_moves`, `timeout`, `disconnect`, `protocol`, `no_show`, voir [formats.md, Objet résultat](formats.md#objet-résultat)).

```json
[
  { "team": "eq200", "games": 10375, "wins": 6965,
    "red": { "games": 5188, "wins": 3486 }, "black": { "games": 5187, "wins": 3479 },
    "losses": { "no_moves": 3368, "timeout": 33, "no_show": 7, "disconnect": 2 } }
]
```

### `head-to-head` : face-à-face

Une entrée par paire d'équipes qui se sont rencontrées, `a` avant `b` dans l'ordre alphabétique.

```json
[ { "a": "eq200", "b": "eq417", "games": 412, "a_wins": 230, "b_wins": 182 } ]
```

### `pairs?a=<équipe>&b=<équipe>` : paires d'un duel

Les paires complètes du ladder entre `a` et `b` : leurs deux parties ont la même ouverture et des couleurs inversées (colonne `pair`, voir [formats.md](formats.md)). La plus récente d'abord ; tournois exclus ; `session` ignoré. `build_a` et `build_b`, facultatifs, retiennent les paires dont les deux parties ont été jouées avec ces builds ; sans eux, toutes les versions. `a` ou `b` manquant : 400. La page Duel en tire le test du signe, le SPRT et le score.

```json
[ { "games": ["20261118T101500.000Z-eq07-eq12", "20261118T101530.000Z-eq12-eq07"],
    "winners": ["eq07", "eq07"], "opening": ["G1-I1-C12", "F12-D10-F1"] } ]
```

### `openings` : ouvertures et premiers coups

- `openings` : les 50 ouvertures imposées les plus jouées (demi-coups tirés par le ladder, voir `opening_plies`), avec les victoires du rouge.
- `first_moves` : pour chaque couleur, les 50 premiers coups clients les plus joués, avec les victoires de la couleur qui les a joués.

```json
{
  "openings": [ { "opening": ["G1-E3-I12", "F12-E11-H3"], "games": 34, "red_wins": 18 } ],
  "first_moves": {
    "red": [ { "move": "I1-F4-A7", "games": 210, "wins": 131 } ],
    "black": [ { "move": "D10-C10-I7", "games": 187, "wins": 90 } ]
  }
}
```

### `builds?team=<équipe>` : par version de jar

Le bilan de chaque build (16 premiers caractères hexadécimaux du sha256 du jar) de l'équipe, dans l'ordre où ils ont joué pour la première fois. Utile pour voir si une soumission a amélioré les résultats. Les parties sans build connu (bots de `[[team]]`) n'y sont pas.

```json
[
  { "build": "812198fd4ad7b97e", "games": 5551, "wins": 2472,
    "first": "2026-11-02T02:32:24.336Z", "last": "2026-11-08T23:41:14.721Z" }
]
```

### `ratings?team=<équipe>` : courbe Elo

Le rating de l'équipe après chacune de ses parties classées, dans l'ordre où elles ont été jouées : `all` toutes sessions confondues, `in_session` dans la session de la partie.

```json
[
  { "game": "20261111T161123.521Z-eq100-eq214", "ts": "2026-11-11T16:11:23.521Z", "session": "automne-2026",
    "all": { "rating": 1532.4, "delta": 6.1 }, "in_session": { "rating": 1518.0, "delta": 7.3 } }
]
```

### `move-times?team=<équipe>` : temps de réflexion

Par build, dans l'ordre où ils ont joué pour la première fois : nombre de coups chronométrés, moyenne et maximum en ms, et `near_limit`, le nombre de coups à 90 % ou plus du temps par coup (`time_ms`). Le premier coup de chaque joueur n'entre pas dans `near_limit` : il a une seconde de grâce pour la JVM à froid. `build` vaut `null` pour les bots de `[[team]]`.

```json
[ { "build": "812198fd4ad7b97e", "moves": 182340, "avg_ms": 212, "max_ms": 1487, "near_limit": 31 } ]
```

### `heatmap?team=<équipe>` : cartes de chaleur

Par build, dans l'ordre où ils ont joué pour la première fois, les cartes de l'arène isola : `moves` compte, par case, les arrivées du pion de l'équipe, `blocks` les cases qu'elle a détruites. Coups clients seulement, sans l'ouverture tirée par le ladder ; une case jamais touchée n'y figure pas. `games` compte les parties où le build a joué au moins un coup. `build` vaut `null` pour les bots de `[[team]]`. La page Analyse en dessine les deux versions de la partie.

```json
[ { "build": "812198fd4ad7b97e", "games": 5551, "moves": { "F4": 812, "G6": 1290 }, "blocks": { "A7": 95, "F10": 431 } } ]
```

### `submissions?team=<équipe>` : soumissions

Les jars soumis par l'équipe, du plus ancien au plus récent, avec leur verdict (`accepted`, `rejected` ou `error`). `detail` donne la raison d'un rejet ; il est vide pour une erreur du ladder, dont le message peut contenir des chemins du serveur. `session` n'a pas d'effet ici.

```json
[
  { "ts": "2026-11-02T02:30:11.204Z", "hash": "812198fd4ad7b97e", "verdict": "accepted", "detail": "" },
  { "ts": "2026-11-05T18:02:47.950Z", "hash": "0b4e19c2a7d3f561", "verdict": "rejected", "detail": "en rouge : temps dépassé" }
]
```

| Code | Sens |
| --- | --- |
### `template-share?team=<équipe>&build=<build>` : part du gabarit

Sur les parties profilées d'un build, la part des échantillons où une méthode du code de départ du cours (`crates/isolation-ladder/src/template.txt`) est en haut de la pile, en pour cent au dixième ; `others` est le reste. Servie seulement tant que l'équipe publie ce build (`"public": true` dans `PUT /api/profiling`), sinon 404 : ni arbre ni profondeur, qui trahiraient la force de sa recherche. Les chiffres viennent du jar, qui pourrait les fausser : c'est une part déclarée. `session` n'a pas d'effet ici.

```json
{ "team": "eq07", "build": "3f9c2a1b5d7e9f01", "games": 12, "samples": 29760,
  "methods": { "Client.main": 0.4, "GameConnection.readMessage": 1.2 }, "others": 98.4 }
```

| Code | Sens |
| --- | --- |
| 200 | `application/json`. |
| 400 | `builds`, `ratings`, `move-times`, `heatmap` ou `submissions` sans `team`, `template-share` sans `team` ou `build`, ou paramètres illisibles. |
| 404 | Statistique inconnue (route inconnue), ou part du gabarit non publiée. |

## Requêtes libres

Pour une question que ces routes ne couvrent pas, la base s'interroge directement, sur le serveur ou sur une copie (voir [deploy/README.md](../deploy/README.md#au-quotidien)) :

```sh
sqlite3 -readonly state/ladder.db \
  "SELECT reason, count(*) FROM games WHERE session = 'automne-2026' GROUP BY reason"
```

Le schéma est dans `crates/isolation-ladder/src/sql/schema.sql` et décrit dans [formats.md, Base des parties](formats.md#base-des-parties-stateladderdb).
