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
- [ ] ROMs multi-fichiers (zip à extraire)
- [ ] Synchronisation des sauvegardes avec RomM (`/api/saves`)
- [ ] Installation automatique des émulateurs (assistant au premier lancement)
- [ ] Choix de l'émulateur par plateforme
- [ ] CI de build Windows + macOS
