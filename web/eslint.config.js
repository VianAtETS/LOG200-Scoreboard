import tsParser from '@typescript-eslint/parser';
import { configs } from 'eslint-plugin-astro';

export default [
  {
    ignores: ['dist/**', '.astro/**', 'node_modules/**', '**/*.{js,mjs,cjs,ts,tsx,jsx}'],
  },
  ...configs['flat/all'],
  ...configs['flat/jsx-a11y-strict'],
  {
    files: ['**/*.astro'],
    linterOptions: {
      reportUnusedDisableDirectives: 'error',
    },
    languageOptions: {
      parserOptions: { parser: tsParser },
    },
  },
];
