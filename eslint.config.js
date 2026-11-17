import js from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default defineConfig([
  globalIgnores(['dist', 'coverage', 'target', 'src-tauri', 'node_modules']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.strictTypeChecked,
      tseslint.configs.stylisticTypeChecked,
      reactHooks.configs.flat['recommended-latest'],
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2023,
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/consistent-type-imports': 'error',
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
      // Aucun rendu HTML brut : protège contre l'injection de contenu venant d'un serveur.
      'no-restricted-syntax': [
        'error',
        {
          selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']",
          message: 'Rendu HTML brut interdit (voir .claude/rules/frontend.md).',
        },
      ],
      // Tous les appels au backend passent par src/lib/ipc.ts.
      'no-restricted-imports': [
        'error',
        {
          paths: [
            {
              name: '@tauri-apps/api',
              message: 'Passer par les fonctions typées de src/lib/ipc.ts.',
            },
          ],
          patterns: [
            {
              group: ['@tauri-apps/api/*', '@tauri-apps/plugin-*'],
              message: 'Passer par les fonctions typées de src/lib/ipc.ts.',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['src/lib/ipc.ts'],
    rules: { 'no-restricted-imports': 'off' },
  },
  {
    files: ['src/components/ui/**/*.tsx'],
    // Les composants shadcn exportent aussi leurs variantes (cva).
    rules: { 'react-refresh/only-export-components': 'off' },
  },
  {
    files: ['isolation/**/*.js'],
    extends: [js.configs.recommended],
    languageOptions: {
      sourceType: 'script',
      globals: { ...globals.browser, module: 'writable' },
    },
  },
  {
    files: ['*.config.{js,ts}', 'eslint.config.js'],
    languageOptions: { globals: globals.node },
  },
]);
