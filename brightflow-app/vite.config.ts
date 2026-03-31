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
    sortTailwindcss: {
      stylesheet: './src/assets/main.css',
      attributes: [':class'],
    },
  },
  lint: {
    ignorePatterns: ['dist/**', 'src/types/generated/**'],
    options: {
      typeAware: true,
      typeCheck: true,
    },
    rules: {
      'no-console': 'warn',
      'no-debugger': 'error',
      eqeqeq: ['error', 'smart'],
      'no-shadow': 'error',
      'require-await': 'error',
      '@typescript-eslint/strict-boolean-expressions': 'error',
      '@typescript-eslint/no-unsafe-type-assertion': 'error',
      'unicorn/consistent-function-scoping': 'error',
      'unicorn/custom-error-definition': 'error',

      // --- Advisory: complexity metrics (warn = visible but non-blocking) ---
      'max-statements': ['warn', { max: 25 }],
      'max-lines-per-function': ['warn', { max: 150 }],
      'max-lines': 'warn',

      // --- Off: Vue/JS idiom conflicts ---
      'func-style': 'off', // Function declarations idiomatic in Vue/Pinia
      'unicorn/no-null': 'off', // Vue ref<T | null>(null) is standard
      'unicorn/filename-case': 'off', // PascalCase SFCs = official Vue convention

      // --- Off: too aggressive ---
      'no-magic-numbers': 'off', // Flags 0, 1024, 3600 etc.
      'id-length': 'off', // Flags (a, b) in sort comparators
      'sort-keys': 'off', // Destroys logical key grouping
      'no-ternary': 'off', // Bans a core language feature
      'no-inline-comments': 'off', // Marginal style preference
      'prefer-destructuring': 'off', // Splice()[0] reads more clearly

      // --- Off: handled elsewhere or not applicable ---
      'sort-imports': 'off', // Handled by Oxfmt sortImports
      'unicorn/prefer-global-this': 'off', // Browser-only app
      'unicorn/prefer-add-event-listener': 'off', // WebSocket .onopen is idiomatic
      'no-negated-condition': 'off', // Only 2 instances, both clear
      'unicorn/no-immediate-mutation': 'off', // Clone-then-mutate is a clear pattern
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
  staged: {
    '*.{ts,vue}': 'vp check --fix',
  },
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    forwardConsole: true,
  },
});
