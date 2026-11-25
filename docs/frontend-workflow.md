# Frontend workflow

Run every command from `web/` with Node 26.13.1 and Bun 1.4.5.
Install with `bun install --frozen-lockfile`.

Node 26.13.1 stays required alongside Bun because `@astrojs/ts-content-mapper` invokes `node` directly, `tsrs-lint` and the `.bin/*` CLI wrappers/LSPs use `#!/usr/bin/env node`, and `dev:golden` relies on Node's `http` upgrade handling.

## Commands

Each bare command runs the whole family. Each suffixed command runs one member.

| Command                   | What it runs                                                                                                               |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `bun run lint`            | Full Oxlint and ESLint Astro in parallel.                                                                                  |
| `bun run lint:fast`       | Oxlint plus ESLint Astro in parallel, without typed lint. Use this for quick feedback.                                     |
| `bun run lint:ts`         | Oxlint on TS and JS files only.                                                                                            |
| `bun run lint:astro`      | ESLint on Astro files only. Oxlint cannot read Astro templates.                                                            |
| `bun run lint:typed`      | Full Oxlint pass with native rules, React Doctor, TypeScript-aware rules and unused-directive checks.                      |
| `bun run lint:fix`        | Apply Oxlint automatic fixes with typed rules enabled.                                                                     |
| `bun run format`          | Check formatting with Oxfmt. It writes nothing.                                                                            |
| `bun run format:fix`      | Rewrite files with Oxfmt. Run `bun run format` first and review the diff.                                                  |
| `bun run test`            | App tests with Bun, 4 workers, no isolation, concurrency 60.                                                               |
| `bun run test:watch`      | Rerun app tests on every save.                                                                                             |
| `bun run typecheck`       | Every typecheck. Sync Astro types once, then run Bun check and the full Astro checker in parallel.                         |
| `bun run typecheck:ts`    | Fast ordinary check only. It cannot see Astro template errors.                                                             |
| `bun run typecheck:astro` | Full check with the pinned native compiler and the Astro mapper. This is the reference.                                    |
| `bun run typecheck:watch` | Rerun the full check on every save. Restart it when Astro types change shape.                                              |
| `bun run suite:quick`     | Oxlint, Astro lint, format check and tests in parallel.                                                                    |
| `bun run suite`           | Sync Astro types, then run the linters, format check, both typechecks, tests and audit in parallel. Build after they pass. |
| `bun run audit`           | Fail on high severity dependency issues.                                                                                   |

## Structured diagnostics

For a full machine-readable lint report, run these commands from `web/`.
Each report stays valid JSON when the tool finds violations and exits with an
error.

```sh
bun run lint:typed --format json
bun run lint:astro --format json
```

To check just the files you changed, pass their paths to the local binaries.
Keep the typed config and unused-directive check enabled on the Oxlint pass.
Run `bun run suite` before requesting review.

```sh
./node_modules/.bin/oxlint -c .oxlintrc.typed.json --type-aware --deny-warnings \
  --report-unused-disable-directives-severity=error src/app/classement.tsx
./node_modules/.bin/eslint src/pages/index.astro
```

## Why the pinned native compiler stays

`bun check` is fast but blind to Astro templates. A probe with three planted
Astro errors proved it: the mapped checker reported all three, `bun check`
reported none. So `typecheck:ts` is a quick signal, never the reference.

The Astro mapper plugs into TypeScript through the `contentMapper` API. That
API only exists in native TypeScript 7.1 nightly builds, not in stable 7.0.
So `typecheck:astro` pins `typescript@7.1.0-dev.20261125.1` for that one
command and nothing else.

ESLint Astro rules need the classic JS parser API. The native compiler does
not offer it yet. So classic TypeScript 6.0.3 stays installed for the lint
parser only.

Editors follow the same split. VS Code uses the Native Preview extension with
`useTsgo`. Parity with the pinned CLI and the Astro mapper is unverified, so
the CLI check stays the reference. Zed keeps `vtsls`.

## Rules

Fix lint findings in the source. When following a rule at a specific line would
make correct code less safe or harder to understand, use the smallest possible
directive immediately above that line. Name the rule and explain the invariant
that makes the exception necessary. This example documents a sequential API.

```ts
// oxlint-disable-next-line eslint/no-await-in-loop -- Each request depends on the previous response.
const page = await fetchNextPage(cursor);
```

An exception across the project requires a demonstrated incompatibility with
the language, framework or runtime. Document the evidence and keep the scope
as small as possible. For example, the automatic JSX runtime supplies the JSX
helpers, so `react/react-in-jsx-scope` does not apply. TypeScript files use
`typescript/require-await` in the full pass because it can check promise returns.
The syntax-only `eslint/require-await` remains enabled for JavaScript.

### Existing global exclusions

The current profiles also contain these inherited exclusions. The categories
below record the rationale; a project preference is not a technical incompatibility.
A new global exception still requires the evidence described above.

| Excluded rule or group                                                                                          | Basis                                                                                  |
| --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `react/react-in-jsx-scope`                                                                                      | Automatic JSX runtime provides the helpers.                                            |
| `import/no-named-export`, `import/prefer-default-export`                                                        | Conflict with the active named-export convention and `import/no-default-export`.       |
| `oxc/no-async-await`, `oxc/no-optional-chaining`                                                                | Conflict with active rules requiring async functions and preferring optional chaining. |
| `node/no-top-level-await`                                                                                       | Top-level await is supported by the project's ESM environment.                         |
| `oxc/no-rest-spread-properties`, `react/jsx-no-literals`                                                        | Project permits object and prop spread and literal component text.                     |
| `import/group-exports`, `import/exports-last`                                                                   | Export placement preference.                                                           |
| `eslint/no-ternary`, `eslint/no-undefined`, `eslint/init-declarations`, `unicorn/no-null`, `eslint/no-continue` | The profile permits these language constructs.                                         |
| `eslint/sort-keys`, `eslint/capitalized-comments`                                                               | Ordering and comment style preferences; comments also contain French and identifiers.  |
| `typescript/prefer-readonly-parameter-types`                                                                    | Typing preference; no general incompatibility established.                             |

The full Oxlint pass checks unused directives with the typed rules enabled.
The quick pass gives fast syntax-based feedback while you edit.
The full pass uses two Oxlint threads. With the current project size that
setting took less wall time and less peak memory than automatic thread
selection in local measurements, with identical rules and file coverage.

## CI and deployments

The `web` job runs `bun run suite` on Ubuntu with one checkout, one frozen
install and a cache for Bun package downloads. It runs alongside the Rust and
repository checks. The `gate` job reports the result required for merge.

Pages and Docker run the full typecheck before the raw build.
