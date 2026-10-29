# Soumettre son jar automatiquement

[`ladder.yml`](ladder.yml) est un workflow GitHub Actions pour le dépôt de votre équipe. À chaque push sur `main` qui modifie `src/`, il compile votre client, construit le jar et l'envoie au ladder. Vous n'avez plus rien à faire à la main : votre dernière version joue contre les autres équipes et apparaît au classement.

```mermaid
flowchart LR
    Push["git push<br>sur main"] --> CI["GitHub Actions<br>compile src/*.java<br>construit client.jar"]
    CI --> API["Ladder<br>PUT /api/submit"]
    API --> Smoke{"Smoke test<br>contre le bot aléatoire"}
    Smoke -- refusé --> Rouge["Run en échec<br>raison dans le résumé"]
    Smoke -- accepté --> Classement["Votre jar joue<br>dès la partie suivante"]
```

## Ce qu'il faut

- Le code Java dans `src/`, sans package, avec une classe `Client` qui contient le `main`.
- Le **token** de votre équipe, donné en privé par l'enseignant.
- L'**adresse** de la page du ladder, par exemple `https://ladder.example.org`.

## Installation

1. Copier `ladder.yml` dans le dépôt de l'équipe, sous `.github/workflows/ladder.yml`, puis le committer.
2. Sur GitHub, ouvrir **Settings > Secrets and variables > Actions**.
3. Onglet **Secrets**, bouton **New repository secret** : nom `LADDER_TOKEN`, valeur le token de l'équipe.
4. Onglet **Variables**, bouton **New repository variable** : nom `LADDER_URL`, valeur l'adresse de la page du ladder.

| Nom | Où | Valeur |
| --- | --- | --- |
| `LADDER_TOKEN` | Secrets | token de l'équipe (ne jamais le committer) |
| `LADDER_URL` | Variables | adresse de la page du ladder |

Tant que `LADDER_URL` n'est pas réglée, le workflow est sauté : il peut rester dans le dépôt sans rien casser.

## Soumettre

- **Automatiquement** : pousser sur `main` un commit qui modifie `src/`.
- **À la main** : onglet **Actions**, workflow **Ladder**, bouton **Run workflow**.

Le smoke test joue deux parties et attend la fin de la partie en cours du ladder : un run prend quelques minutes. Le verdict s'affiche dans le **résumé du run** (onglet Actions, cliquer sur le run), avec un lien vers le classement.

## Lire le verdict

| Résumé du run | Sens | Que faire |
| --- | --- | --- |
| `HTTP 200` accepté | Nouvelle version en jeu dès la prochaine partie | Rien |
| `HTTP 200` déjà actif | Ce jar est déjà celui qui joue (même commit relancé) | Rien |
| `HTTP 400` | `Ladder-Threads` du manifeste ne vaut ni 1 ni 2 (jar construit à la main) | Le corriger, ou passer par `MULTITHREAD` dans `ladder.yml` |
| `HTTP 401` | Token absent ou faux | Vérifier le secret `LADDER_TOKEN` |
| `HTTP 413` | Jar trop gros | Retirer les fichiers inutiles de `src/` |
| `HTTP 422` | Refusé par le smoke test : dépassement de temps, déconnexion, absence ou coup invalide. L'ancienne version continue de jouer | Lire la raison, tester en local (voir plus bas), corriger |
| `HTTP 429` | Soumission trop rapprochée de la précédente | Attendre le délai indiqué, puis **Run workflow** |
| `HTTP 500` | Erreur du ladder | Réessayer plus tard |
| `HTTP 000` | Ladder injoignable | Vérifier `LADDER_URL` ; sinon, soumettre par le formulaire de la page |

Perdre le smoke test n'est pas un refus : seul compte le fait de jouer jusqu'au bout avec des coups valides.

## Obfusquer le jar (optionnel)

Dans `ladder.yml`, remplacer `OBFUSCATE: false` par `OBFUSCATE: true` (en haut du fichier) : le workflow passe vos classes dans [ProGuard](https://github.com/Guardsquare/proguard) avant de construire le jar : classes, méthodes et champs deviennent `a`, `b`, `c`… Seul `Client.main` garde son nom. Le jar se comporte exactement pareil et ne joue pas plus lentement.

C'est une gêne, pas une protection : un décompilateur ou une IA retrouve quand même la logique du code.

Les stack traces du jar affichent alors les noms obfusqués, par exemple dans la raison d'un refus 422. Chaque run garde l'artefact `proguard-mapping` (le fichier `mapping.txt`), qui les retraduit avec l'outil `retrace` de ProGuard :

```sh
java -jar proguard-7.10.0/lib/retrace.jar mapping.txt trace.txt
```

Si votre code charge des classes par leur nom (réflexion, `Class.forName`), le renommage le casse : le smoke test refusera le jar, et l'ancienne version continuera de jouer.

## Bot multithread (optionnel)

Par défaut, votre bot a **1 cœur**. Des threads en plus se le partagent et ne font rien gagner.

Dans `ladder.yml`, remplacer `MULTITHREAD: false` par `MULTITHREAD: true` : le jar déclare `Ladder-Threads: 2` dans son manifeste, et toutes vos parties se jouent sur **2 cœurs par couleur**. Votre adversaire a les mêmes 2 cœurs, pour que la partie reste équitable. En échange, ces parties se jouent sur un emplacement réservé, partagé avec les autres équipes multithread : vous jouez **moins souvent**, et votre classement se stabilise plus lentement. N'activez l'option que si votre bot calcule vraiment en parallèle.

Le smoke test, lui, se joue toujours sur 1 cœur. Calibrez donc le nombre de threads sur ce que la JVM voit, plutôt que de le coder en dur :

```java
int threads = Runtime.getRuntime().availableProcessors(); // 1 au smoke test, 2 sur les parties multithread
```

La déclaration suit le jar : si la version suivante est construite avec `MULTITHREAD: false`, elle rejoue sur 1 cœur. Sans la CI, ajoutez la ligne `Ladder-Threads: 2` au manifeste vous-même (`jar --create --manifest ...`).

## Jouer sans téléverser son jar (optionnel)

Votre jar peut aussi jouer depuis votre ordinateur : il n'est jamais envoyé au serveur, seuls ses coups le sont. Téléchargez l'archive de votre système depuis la [dernière release](https://github.com/VianAtETS/LOG200-Scoreboard/releases/latest), puis :

```sh
LADDER_TOKEN=<token> ./isolation-remote --url <adresse de la page> -- java -jar Client-eqXX.jar
```

Sous Windows (PowerShell) : `$env:LADDER_TOKEN="<token>"`, puis `.\isolation-remote.exe --url ... -- java -jar Client-eqXX.jar`.

Tant que la commande tourne, votre équipe est au tirage : le relais lance votre jar pour chaque partie, et il se connecte à `localhost:8888` comme d'habitude. Fermez-la (Ctrl+C) pour arrêter ; si vous avez aussi soumis un jar, c'est lui qui reprend.

- **La latence compte dans votre temps par coup.** Le relais annonce à votre jar un temps réduit d'autant dans le message `0` : lisez-le plutôt que de coder 500 ms en dur.
- **Vos parties portent le badge "distant".** Le serveur ne vérifie ni votre code ni votre machine : la page permet de les retirer du classement ("Vérifiées seulement").
- **Une partie commencée se joue.** Couper le relais pendant une partie la fait perdre (`disconnect`), et la suivante de la paire aussi (`no_show`).

## Tester avant de pousser

Un refus 422 se reproduit en local avec l'arbitre et le bot de la [dernière release](https://github.com/VianAtETS/LOG200-Scoreboard/releases/latest) : voir "Test your bot" dans le [README principal](../README.md#test-your-bot).

## Sans la CI

Le formulaire de la page du ladder fait la même chose, tout comme `curl` :

```sh
curl -T client.jar -H "Authorization: Bearer $LADDER_TOKEN" "$LADDER_URL/api/submit"
```

Détails du protocole de soumission : "Soumission des jars" dans [`docs/formats.md`](../docs/formats.md#soumission-des-jars).
