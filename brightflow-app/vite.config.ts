import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import ui from '@nuxt/ui/vite';
import { consoleForwardPlugin } from './vite-console-forward-plugin';
import { fileURLToPath, URL } from 'node:url';

export default defineConfig({
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
    consoleForwardPlugin(),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
});
