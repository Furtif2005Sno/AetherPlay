# RomM Desktop

Client desktop (Windows / macOS Apple Silicon) pour [RomM](https://github.com/rommapp/romm) :
parcourir sa bibliothèque, cliquer sur **Jouer**, la ROM est téléchargée puis lancée dans l'émulateur adapté.

Stack : **Tauri 2** (backend Rust) + Vue 3, avec les design tokens du front RomM.

## Développement

```sh
npm install
npm run tauri dev      # lancer en dev
npm run tauri build    # générer l'installeur
```

## Structure

- `src-tauri/src/romm.rs`: client API RomM (auth Basic, plateformes, ROMs, téléchargement, covers)
- `src-tauri/src/emulators.rs` + `catalog.json`: catalogue d'émulateurs, choix par plateforme, lancement
- `src-tauri/src/config.rs`: réglages (mot de passe stocké dans le trousseau de l'OS)
- `src/`: interface Vue (connexion, bibliothèque, réglages émulateurs)

## Avancement

- [x] Connexion au serveur RomM, mot de passe dans le trousseau de l'OS
- [x] Bibliothèque par plateforme, covers, recherche
- [x] Téléchargement avec progression + cache local
- [x] Lancement de l'émulateur configuré (RetroArch avec cores par plateforme)
- [x] ROMs multi-fichiers : zip extrait, choix auto du fichier (m3u/cue/chd/iso…, EBOOT.BIN PS3, plus gros .nsp Switch), .m3u généré pour les multi-disques
- [x] Synchronisation des sauvegardes avec RomM (restauration avant la partie, envoi après) : dossiers par émulateur dans `catalog.json`, à vérifier sur chaque émulateur
- [x] Installation automatique des émulateurs (assistant au premier lancement, sources dans `sources.json`) ; cores RetroArch téléchargés à la demande. Dolphin et Eden : installation manuelle
- [ ] Choix de l'émulateur par plateforme
- [ ] CI de build Windows + macOS
