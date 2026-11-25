// Serveur : la balise meta scoreboard-server (posée par le workflow GitHub Pages), sinon le site qui
// sert la page. Données : ?data=URL du dossier, sinon <serveur>/data.

const meta = (): string | undefined =>
  document.querySelector<HTMLMetaElement>('meta[name="scoreboard-server"]')?.content;

const serverMeta = meta();
export const SERVER = serverMeta === undefined || serverMeta === '' ? '..' : serverMeta;
export const DATA = new URLSearchParams(location.search).get('data') ?? `${SERVER}/data`;
export const API = `${SERVER}/api`;

export const REFRESH_MS = 15_000;

export const LATEST_RELEASE =
  'https://api.github.com/repos/VianAtETS/LOG200-Scoreboard/releases/latest';
