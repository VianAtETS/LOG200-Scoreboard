// Serveur : la balise meta scoreboard-server (posée par le workflow GitHub Pages), sinon le site qui
// sert la page. Données : ?data=URL du dossier, sinon <serveur>/data.

const meta = () =>
  document.querySelector<HTMLMetaElement>('meta[name="scoreboard-server"]')?.content;

export const SERVER = meta() || '..';
export const DATA = new URLSearchParams(location.search).get('data') ?? `${SERVER}/data`;
export const API = `${SERVER}/api`;

export const REFRESH_MS = 15000;

export const LATEST_RELEASE =
  'https://api.github.com/repos/VianAtETS/LOG200-Scoreboard/releases/latest';
