// oxlint-disable-next-line unicorn/require-module-specifiers -- This browser entry point has module scope and no public exports.
export {};

const configuredServer = document.querySelector('meta[name="scoreboard-server"]')?.content;
const server = configuredServer === undefined || configuredServer === '' ? '..' : configuredServer;

// Swagger UI a un thème sombre, activé par une classe sur <html> : suivre le système comme le site.
const dark = matchMedia('(prefers-color-scheme: dark)');

const theme = () => {
  document.documentElement.classList.toggle('dark-mode', dark.matches);
};

theme();

dark.addEventListener('change', theme);

// POST n'est pas autorisé par le préflight CORS de Caddy : PUT suffit pour soumettre.
const initSwagger = globalThis.SwaggerUIBundle;

initSwagger?.({
  url: `${server}/api/openapi.json`,
  dom_id: '#swagger',
  supportedSubmitMethods: ['get', 'put'],
});
