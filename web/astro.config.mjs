import react from '@astrojs/react';
import { defineConfig } from 'astro/config';
import { fileURLToPath } from 'node:url';

// Ladder lu en dev : /api et /data sont relayés vers lui. SCOREBOARD_DEV_SERVER pour en viser un autre.
const DEV_SERVER = process.env.SCOREBOARD_DEV_SERVER ?? 'http://localhost:8080';

// Contours ajoutés aux éléments de la page (dev/layout-debug.css), en dev seulement : bouton « Layout »
// dans la barre de dev d'Astro.
const layoutDebug = {
  name: 'layout-debug',
  hooks: {
    'astro:config:setup': ({ command, addDevToolbarApp }) => {
      if (command !== 'dev') {
        return;
      }

      addDevToolbarApp({
        id: 'layout-debug',
        name: 'Layout',
        icon: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2" stroke-dasharray="4 3"/><rect x="8" y="8" width="8" height="8"/></svg>',
        entrypoint: fileURLToPath(new URL('dev/layout-debug-app.js', import.meta.url)),
      });
    },
  },
};

// https://astro.build/config
export default defineConfig({
  integrations: [react(), layoutDebug],
  // Pages en fichiers (graphe.html, tournoi.html...) : les anciens liens restent valides.
  build: {
    format: 'file',
    // Chemins relatifs : la page se sert telle quelle sous /web/ (Docker) comme sous /<dépôt>/ (GitHub Pages).
    assetsPrefix: '.',
  },
  server: {
    port: 3000,
  },
  vite: {
    server: {
      proxy: {
        // Pas /api.html : seulement ce qui est sous /api/ et /data/.
        '^/api/': { target: DEV_SERVER, changeOrigin: true, ws: true },
        '^/data/': { target: DEV_SERVER, changeOrigin: true },
      },
    },
  },
});
