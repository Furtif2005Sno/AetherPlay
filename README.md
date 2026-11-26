# AetherPlay

Client desktop (Windows / macOS Apple Silicon) pour [RomM](https://github.com/rommapp/romm) : l'interface est celle de RomM elle-même (embarquée depuis RomM 5.4.1, voir `romm-ui/AETHERPLAY.md`), et le bouton **Jouer** télécharge la ROM puis la lance dans un émulateur local, avec synchronisation des sauvegardes.

Stack : **Tauri 2** (backend Rust). AetherPlay implémente `window.rommNative`, le contrat « desktop shell » prévu par RomM.

Licence : **AGPL-3.0** (comme RomM, dont l'interface est reprise).

## Télécharger

- **Windows** : [AetherPlay-Windows.msi](https://github.com/Furtif2005Sno/AetherPlay/releases/latest/download/AetherPlay-Windows.msi)
- **Mac** (Apple Silicon) : [AetherPlay-Mac.dmg](https://github.com/Furtif2005Sno/AetherPlay/releases/latest/download/AetherPlay-Mac.dmg)
- **SteamOS / Linux** (Steam Deck, ROG Ally…) : [AetherPlay-SteamOS.AppImage](https://github.com/Furtif2005Sno/AetherPlay/releases/latest/download/AetherPlay-SteamOS.AppImage)
- **Android** (le téléphone comme manette) : [AetherPlay-Android.apk](https://github.com/Furtif2005Sno/AetherPlay/releases/latest/download/AetherPlay-Android.apk)

Ces liens donnent toujours la dernière version.

## Développement

```sh
npm install
npm --prefix romm-ui ci
npm --prefix romm-ui run build
npm run tauri dev      # lancer en dev
npm run tauri build    # générer l'installeur
```

## Structure

- `romm-ui/`: interface de RomM 5.4.1 embarquée (voir `romm-ui/AETHERPLAY.md`)
- `src-tauri/src/shell.rs` + `bridge.js`: sert l'interface RomM sur `romm://`, relaie l'API vers le serveur, fournit `window.rommNative`
- `src-tauri/src/romm.rs`: client API RomM (ROM, téléchargement, sauvegardes)
- `src-tauri/src/emulators.rs` + `catalog.json`: catalogue d'émulateurs, choix par plateforme, lancement
- `src-tauri/src/config.rs`: réglages (mot de passe stocké dans le trousseau de l'OS)
- `src/`: pages propres à AetherPlay (adresse du serveur, assistant d'émulateurs, fenêtre de réglages : émulateurs, stockage)

## Avancement

- [x] Connexion au serveur RomM, mot de passe dans le trousseau de l'OS
- [x] Bibliothèque par plateforme, covers, recherche
- [x] Téléchargement avec progression + cache local
- [x] Lancement de l'émulateur configuré (RetroArch avec cores par plateforme)
- [x] ROMs multi-fichiers : zip extrait, choix auto du fichier (m3u/cue/chd/iso…, EBOOT.BIN PS3, plus gros .nsp Switch), .m3u généré pour les multi-disques
- [x] Synchronisation des sauvegardes avec RomM (restauration avant la partie, envoi après) : dossiers par émulateur dans `catalog.json`, à vérifier sur chaque émulateur
- [x] Installation automatique des émulateurs (assistant au premier lancement, sources dans `sources.json`) ; cores RetroArch téléchargés à la demande
- [x] Choix de l'émulateur par console, dossiers de saves modifiables
- [x] Mises à jour/DLC Switch chargées sans installation (Eden/Eglantine, `external_content_dirs`)
- [x] Gestion du cache des ROMs (onglet Stockage)
- [x] CI de build Windows + macOS + Linux (AppImage), release avec liens directs
- [x] Interface pilotable à la manette (navigation RomM ; manette lue par AetherPlay sur macOS/Linux), Select+Start pour quitter un jeu
- [x] Joueur 1 d'Eden/Eglantine réglé automatiquement sur la manette branchée

## Manette Android et RetroTogether

L'app Android (`android/`, protocole dans `android/PROTOCOL.md`) fait du téléphone une manette :

- **Bluetooth** : le téléphone devient une vraie manette sans fil, pour Windows, Mac ou SteamOS. Première fois : dans l'app, « Rendre le téléphone visible », puis sur l'ordinateur, réglages Bluetooth > Ajouter un appareil > « AetherPlay Manette ».
- **Wi-Fi** : l'app trouve AetherPlay sur le réseau. Tape une seule fois le code affiché dans Réglages > AetherPlay > Manette Android. La manette change d'aspect selon la console du jeu lancé (SNES, N64, PlayStation, Switch…) et affiche la jaquette. Le Wi-Fi pilote aussi les menus et sert de secours sans Bluetooth (manette virtuelle sous SteamOS, ou Windows avec [ViGEmBus](https://github.com/nefarius/ViGEmBus)).
- **Personnaliser** (icône réglages de la manette) : glisser les boutons, les agrandir ou les réduire, réglages gardés par console.
- **Deuxième écran** (Windows) : pour la DS (melonDS) et le GamePad Wii U (Cemu), l'écran tactile s'affiche sur le téléphone au milieu des boutons, et on touche directement dessus. Il suffit que le téléphone soit connecté au lancement du jeu.
- **RetroTogether** : chaque téléphone connecté est un joueur (J1, J2…). AetherPlay attribue les manettes aux joueurs dans l'ordre, pour RetroArch, Eden, Eglantine, PCSX2, DuckStation et Dolphin (GameCube). Cemu, RPCS3, melonDS et Azahar se règlent pour l'instant dans l'émulateur.

## SteamOS : installation

1. Passer en **mode Bureau**, télécharger `AetherPlay-SteamOS.AppImage` (lien ci-dessus) et le ranger, par exemple dans `~/Applications`.
2. Clic droit sur le fichier > Propriétés > Permissions > cocher « Exécutable ».
3. Ouvrir Steam > Jeux > « Ajouter un jeu non-Steam à ma bibliothèque » > Parcourir > choisir l'AppImage.
4. Revenir en **mode Jeu** et lancer AetherPlay depuis la bibliothèque : plein écran, tout se fait à la manette (Steam + X pour le clavier virtuel à la première connexion).

Les émulateurs s'installent depuis Réglages > AetherPlay (versions AppImage), les données restent dans `~/.local/share` et `~/.config`. Le mot de passe RomM est gardé dans `~/.local/share/aetherplay/credentials.json` (lisible par vous seul) car le mode Jeu n'a pas de trousseau.

## macOS : premier lancement

L'app n'est pas notarisée par Apple. Au premier lancement : Réglages Système > Confidentialité et sécurité > « Ouvrir quand même », ou :

```sh
xattr -dr com.apple.quarantine "/Applications/AetherPlay.app"
```
