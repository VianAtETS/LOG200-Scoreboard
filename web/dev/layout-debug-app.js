// Bouton « Layout » de la barre de dev d'Astro : met ou enlève la classe layout-debug sur <html>,
// de dev/layout-debug.css qui affiche des contours aux éléments de la page.

import { defineToolbarApp } from 'astro/toolbar';
import './layout-debug.css';

const KEY = 'layout-debug';
const root = document.documentElement;

const load = () => {
  try {
    return sessionStorage.getItem(KEY);
  } catch (err) {
    console.warn("[layout-debug] échec du chargement de l'état :", err);
    return null;
  }
};

const save = (on) => {
  try {
    sessionStorage.setItem(KEY, on ? '1' : '0');
  } catch (err) {
    console.warn("[layout-debug] échec de la sauvegarde de l'état :", err);
  }
};

export default defineToolbarApp({
  init(_canvas, app) {
    const on = load() === '1';
    root.classList.toggle(KEY, on);
    if (on) app.toggleState({ state: true });
    app.onToggled(({ state }) => {
      root.classList.toggle(KEY, state);
      save(state);
    });
  },
});
