# AetherPlay ↔ app Android

L'app Android est la manette d'AetherPlay. Une seule app, deux liaisons.

## Bluetooth : les touches vers les jeux

Le téléphone se déclare comme manette Bluetooth (profil HID device, Android 9+). L'ordinateur s'y associe comme à n'importe quelle manette sans fil (Windows, macOS, SteamOS), sans pilote.

Rapport HID n° 1, 7 octets :
- 17 boutons dans l'ordre « standard » du W3C, puis 7 bits de bourrage :
  - 0 : bouton du bas, 1 : droite, 2 : gauche, 3 : haut ;
  - 4 : LB, 5 : RB, 6 : LT, 7 : RT ;
  - 8 : Select, 9 : Start, 10 : L3, 11 : R3 ;
  - 12-15 : croix haut / bas / gauche / droite ;
  - 16 : Home.
- 4 axes signés (-127..127) : X et Y du stick gauche, Rx et Ry du stick droit. Y est positif vers le bas.

En plus, deux rapports pour AetherPlay (page « vendor » 0xFF00), qui donnent en Bluetooth seul les mêmes fonctions qu'en Wi-Fi :
- **Rapport de fonctionnalité n° 3** (16 octets) : la signature `AETHERPLAY-PAD01`. AetherPlay la lit pour reconnaître le téléphone parmi les manettes.
- **Rapport de sortie n° 2** (31 octets, du PC vers le téléphone, envoyé seulement quand il change) : version (1), numéro de joueur, console (16 octets ASCII), nom du jeu (13 octets UTF-8). Le téléphone prend la disposition de la console et affiche J1, J2… Le numéro suit l'ordre dans lequel l'émulateur voit les manettes.

Le téléphone répond aux demandes de rapport du PC : l'état de la manette pour le rapport 1, la signature pour le 3. Dans l'app, l'interrupteur « Infos AetherPlay par Bluetooth » permet de revenir à la manette simple (sans ces deux rapports). Après un changement, il faut associer de nouveau le téléphone sur l'ordinateur. Sur le PC, le téléphone apparaît sous son propre nom.

Les navigateurs, donc l'interface RomM, lisent ce rapport tel quel. Pour les émulateurs, AetherPlay ajoute la configuration SDL correspondante (`SDL_GAMECONTROLLERCONFIG`) et un profil RetroArch. Il reconnaît la manette à son nom ou, à défaut, à sa forme (17 boutons, 4 axes, pas de chapeau).

## Wi-Fi : appairage, jeu en cours, menus

- **Découverte :** mDNS `_aetherplay._tcp`, port 47820.
- **Connexion :** WebSocket `ws://<ordinateur>:47820/phone`, messages JSON en texte.

| Sens | Message | Rôle |
|---|---|---|
| téléphone → PC | `{"t":"hello","v":1,"name":"…","token":"…"}` ou `…,"pin":"123456"}` | appairage |
| PC → téléphone | `{"t":"pin"}` | code à taper (affiché dans Réglages > AetherPlay) |
| PC → téléphone | `{"t":"welcome","server":"…","token":"…"}` | appairé ; `token` seulement à la première fois |
| PC → téléphone | `{"t":"denied","retry":true,"reason":"…"}` | mauvais code |
| PC → téléphone | `{"t":"player","n":2}` | RetroTogether : numéro de joueur |
| PC → téléphone | `{"t":"virtual","ok":true}` | le Wi-Fi peut piloter les jeux (manette virtuelle) |
| PC → téléphone | `{"t":"game","platform":"snes","name":"…","cover":"<base64>"}` | jeu lancé : disposition de la console |
| PC → téléphone | `{"t":"idle"}` | retour aux menus |
| téléphone → PC | `{"t":"pad","b":<bits>,"a":[lx,ly,rx,ry],"bt":true}` | état de la manette, envoyé à chaque changement |
| téléphone → PC | `{"t":"quit"}` | fermer le jeu |
| PC → téléphone | `{"t":"screen","on":true,"w":256,"h":192}` / `{"t":"screen","on":false}` | deuxième écran diffusé (Windows) |
| PC → téléphone | message **binaire** : image JPEG | une image du deuxième écran (~30 par seconde) |
| téléphone → PC | `{"t":"touch","x":0.5,"y":0.5,"down":true}` | toucher sur le deuxième écran (0..1), relayé en clic dans la fenêtre de l'émulateur |

L'état reçu en Wi-Fi pilote toujours les menus d'AetherPlay. Il pilote aussi les jeux quand `bt` vaut `false`, via une manette Xbox 360 virtuelle :
- Linux : uinput ;
- Windows : pilote ViGEmBus ;
- macOS : impossible, il faut le Bluetooth.

## Deuxième écran (Windows)

Quand un téléphone est connecté au lancement d'un jeu, AetherPlay ouvre le deuxième écran dans une fenêtre à part :
- melonDS (DS) : `Instance0.Window1` réglée sur « écran du bas seulement », la fenêtre principale gardant celui du haut ;
- Cemu (Wii U) : fenêtre « GamePad View ».

Il la capture avec PrintWindow (même cachée derrière d'autres fenêtres, mais pas réduite), la recadre sur l'écran émulé et l'envoie aux téléphones. Les touchers reviennent en messages souris postés à cette fenêtre : le curseur et le focus ne bougent pas. Sans téléphone, la disposition d'origine est remise.

## RetroTogether

Chaque téléphone connecté est un joueur (J1, J2…) et a sa propre manette, Bluetooth ou virtuelle. Au lancement d'un jeu, AetherPlay attribue les manettes aux joueurs, dans l'ordre :
- RetroArch le fait seul ;
- AetherPlay écrit la configuration d'Eden / Eglantine, PCSX2, DuckStation et Dolphin (GameCube).
