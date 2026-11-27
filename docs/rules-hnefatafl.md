# Hnefatafl 13×13 : règles du serveur officiel

Le laboratoire de LOG320 (énoncé « Jeu de Hnefatafl ») se joue contre un serveur `BoardGame.exe` (Qt, version Hnefatafl). Ses règles et son protocole ont été relevés dans son code, par décompilation (Ghidra) : l'énoncé laisse des points ouverts, et le serveur en tranche certains autrement qu'on le lirait. `crates/hnefatafl` implémente exactement ce qui suit.

## Plateau et notation

- 13×13, colonnes A à M, lignes 1 à 13. Les attaquants (rouges, 24 pions) jouent en premier ; les défenseurs (noirs, 12 pions et le roi) partent du centre, le roi sur le trône G7.
- Attaquants : E13 à I13, G12 ; E1 à I1, G2 ; A5 à A9, B7 ; M5 à M9, L7. Défenseurs : G4 à G10 et D7 à J7, sauf G7.
- Cases spéciales : le trône (G7) et les quatre coins (A1, A13, M1, M13). Elles ne figurent pas dans le plateau du protocole.
- Plateau des messages `1` et `2` : 169 entiers, la ligne 13 d'abord, de A à M dans chaque ligne, chacun suivi d'une espace. Codes : 0 vide, 2 défenseur, 4 attaquant, 5 roi.
- Un coup : case de départ, case d'arrivée. Le serveur écrit `G12 - L12` ; il lit `G12-L12`, `G12 - L12` ou `G12L12` (espaces et `-` en nombre quelconque), **en majuscules seulement**.

## Déplacements

- Toutes les pièces, roi compris, se déplacent comme la tour aux échecs, sans sauter de pièce.
- Seul le roi s'arrête sur le trône ou un coin. Une autre pièce peut passer par-dessus le trône vide.

## Captures

- Après un coup, chacune des quatre pièces voisines de la case d'arrivée est prise si c'est une pièce adverse et que la case au-delà, dans la même direction, contient une pièce alliée, ou est le trône ou un coin.
- **Le trône et les coins sont hostiles à tous, même occupés** : un défenseur encadré par un attaquant et le trône où se trouve le roi est pris.
- Le roi aide à prendre (il compte comme allié des défenseurs) mais **n'est jamais pris ainsi**.
- Le bord ne sert pas à prendre une pièce ordinaire.
- Seules les captures actives comptent : une pièce qui se place entre deux adversaires reste.

## Fin de partie

Après chaque coup, le serveur vérifie, dans cet ordre :

1. **La victoire de celui qui vient de jouer, seulement** :
   - le rouge gagne si le roi est pris : chacun de ses quatre côtés est un attaquant, le trône, un coin ou le bord (deux attaquants suffisent contre un coin, figure de l'énoncé). C'est un état, pas une capture : un roi qui s'enferme lui-même n'est déclaré pris qu'au coup rouge suivant, quel qu'il soit ;
   - le noir gagne si le roi est sur un coin.
2. **La nulle** si l'adversaire n'a plus aucun coup.

Le serveur ne détecte pas les répétitions (l'énoncé le dit : « dans le cas où le jeu n'avance plus, ce sera considéré comme un match nul »). Le ladder en fait une règle : **nulle après 300 demi-coups** (`max_plies`). Raisons enregistrées : `king_captured`, `king_escaped`, `stalemate`, `max_plies`.

## Protocole

Celui de l'ancien serveur du cours (dialecte `Classic` de `game_kit::protocol`), différent de celui d'Isolation :

| Message | Sens |
| --- | --- |
| `2 <plateau>` | Au noir, dès sa connexion. |
| `1 <plateau>` | Au rouge : demande de son premier coup. |
| `3 <coup>` | Demande de coup, avec le dernier coup adverse. |
| `4` | Coup refusé ; le joueur rejoue, son temps court toujours. |
| `5 <coup>` | Fin de partie, avec le dernier coup joué. **Sans résultat**, contrairement à Isolation. |

- Il n'y a pas de message `0` en début de partie, et **aucun message ne se termine par une fin de ligne**. Un client peut envoyer son coup sans fin de ligne : l'arbitre le lit quand il se tait (10 ms).
- Le serveur n'envoie rien aux joueurs pour un temps dépassé ou une nulle (il l'affiche seulement). L'arbitre du ladder envoie `5` dans ces cas aussi, pour que les clients s'arrêtent.
- Temps : 5 s par coup dans l'énoncé, celui des tournois ; 1 s au ladder (`time_ms`). Une partie de 300 demi-coups dure donc au plus 5 min au ladder (le délai d'une partie en tient compte : `max_plies` du descripteur).

## Conformance

`hnefatafl-bot --check` joue contre l'IA du serveur officiel et signale chaque écart avec nos règles : un coup du serveur que nos règles refusent, un des nôtres refusé par `4`, une fin annoncée par `5` que nos règles ne voient pas, ou une partie qui continue alors que nos règles la finissent. Code de sortie 1 s'il y en a eu. `--max-plies N` quitte la partie après N demi-coups : le serveur ne borne pas une partie qui tourne en rond (plusieurs milliers de demi-coups entre son niveau 1 et un bot au hasard), et au-delà de 300 c'est notre limite de nulle, propre au ladder, qui diverge.

Pour la rejouer : le serveur dans le conteneur `server` du devcontainer (copier son dossier dans `official-server/`), le joueur noir ou rouge en « Réseau », l'autre en « Ordinateur », puis

```sh
cargo run -p hnefatafl --bin hnefatafl-bot -- --port 8888 --check --max-plies 290
```
