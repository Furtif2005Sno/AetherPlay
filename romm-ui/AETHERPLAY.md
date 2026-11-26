# Interface RomM embarquée

Ce dossier est le front-end de [RomM](https://github.com/rommapp/romm) **5.4.1**
(`frontend/`), sous licence AGPL-3.0, embarqué tel quel dans AetherPlay.

AetherPlay le sert sur le schéma `romm://` (voir `src-tauri/src/shell.rs`), relaie
ses appels API vers le serveur RomM de l'utilisateur, et lui fournit
`window.rommNative` (`src-tauri/src/bridge.js`), le contrat « desktop shell » de
RomM, pour que les boutons Jouer lancent les jeux dans les émulateurs locaux.

## Modifications par rapport à RomM 5.4.1

- `vite.config.js` : plugins `VitePWA` et `precompress` retirés (pas de service
  worker sur le schéma de l'app, fichiers précompressés inutiles).
- `src/services/socket.ts` : transport `polling` seul (pas de WebSocket sur le schéma de l'app ; le long polling passe par le proxy) ; `src/v2/composables/useSocketTransportHealth` ne signale jamais de connexion WebSocket dégradée dans AetherPlay (bandeau masqué).
- `src/v2/components/AppShell/AppNav.vue`, `src/v2/components/shared/AuthCard.vue` : logo AetherPlay (`public/aetherplay.png`) à la place du logo RomM.
- Page « AetherPlay » dans les réglages (émulateurs, import de sauvegarde Switch, options, stockage, diagnostic) : `src/v2/views/Settings/AetherPlay.vue`, `src/v2/aetherplay/api.ts` ; route `AETHERPLAY` ajoutée dans `src/plugins/routeNames.ts`, `src/plugins/router.ts`, `src/v2/router/routes.ts` ; entrées dans `SettingsSidebar.vue` et `UserMenu.vue`.
- Non copiés : tests, Storybook, e2e.

Chaque modification est marquée d'un commentaire `AetherPlay` dans le code.

## Mettre à jour

1. Remplacer `src/`, `assets/`, `public/`, `scripts/`, `package.json`,
   `package-lock.json` par ceux de la nouvelle version de RomM.
2. Ré-appliquer les modifications ci-dessus.
3. `npm ci && npm run build` dans ce dossier.
