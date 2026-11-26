# AetherPlay

Client desktop (Windows / macOS Apple Silicon) pour [RomM](https://github.com/rommapp/romm) : l'interface est celle de RomM elle-même (embarquée depuis RomM 5.4.1, voir `romm-ui/AETHERPLAY.md`), et le bouton **Jouer** télécharge la ROM puis la lance dans un émulateur local, avec synchronisation des sauvegardes.

Stack : **Tauri 2** (backend Rust). AetherPlay implémente `window.rommNative`, le contrat « desktop shell » prévu par RomM.

Licence : **AGPL-3.0** (comme RomM, dont l'interface est reprise).

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
- [ ] CI de build Windows + macOS

## macOS : premier lancement

L'app n'est pas notarisée par Apple. Au premier lancement : Réglages Système > Confidentialité et sécurité > « Ouvrir quand même », ou :

```sh
xattr -dr com.apple.quarantine "/Applications/AetherPlay.app"
```
