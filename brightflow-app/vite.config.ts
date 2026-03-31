import { URL, fileURLToPath } from 'node:url';

import ui from '@nuxt/ui/vite';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite-plus';

export default defineConfig({
  fmt: {
    ignorePatterns: ['dist/**', 'src/types/generated/**'],
    semi: true,
    singleQuote: true,
    trailingComma: 'all',
    printWidth: 100,
    tabWidth: 2,
    useTabs: false,
    sortImports: {},
  },
  lint: {
    ignorePatterns: ['dist/**', 'src/types/generated/**'],
    options: {
      typeAware: true,
      typeCheck: false, // Tsgolint alpha: Vue SFC resolution issues — re-enable when stable
    },
    rules: {
      'no-console': 'warn',
      'no-debugger': 'error',
      eqeqeq: ['error', 'smart'],
      // Rules new in oxlint 1.57 — disabled to match old 1.43 behavior
      'no-magic-numbers': 'off',
      'func-style': 'off',
      'unicorn/no-null': 'off',
      'id-length': 'off',
      'sort-keys': 'off',
      'unicorn/filename-case': 'off',
      'sort-imports': 'off',
      'no-ternary': 'off',
      'max-statements': 'off',
      'max-lines-per-function': 'off',
      'no-inline-comments': 'off',
      'prefer-destructuring': 'off',
      'no-shadow': 'off',
      '@typescript-eslint/no-unsafe-type-assertion': 'off',
      '@typescript-eslint/strict-boolean-expressions': 'off',
      'require-await': 'off',
      'max-lines': 'off',
      'unicorn/prefer-add-event-listener': 'off',
      'unicorn/prefer-global-this': 'off',
      'no-negated-condition': 'off',
      'unicorn/consistent-function-scoping': 'off',
      'unicorn/custom-error-definition': 'off',
      'unicorn/no-immediate-mutation': 'off',
    },
    categories: {
      correctness: 'error',
      suspicious: 'error',
      pedantic: 'error',
      perf: 'error',
      style: 'error',
    },
  },
  plugins: [
    vue(),
    ui({
      colorMode: true,
      ui: {
        colors: {
          primary: 'purple',
          neutral: 'stone',
        },
      },
    }),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('src', import.meta.url)),
    },
  },
  server: {
    forwardConsole: true,
  },
});
