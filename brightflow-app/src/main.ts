import ui from '@nuxt/ui/vue-plugin';
import { PiniaColada } from '@pinia/colada';
import { createPinia } from 'pinia';
import { createApp } from 'vue';
import { type RouteRecordRaw, createRouter, createWebHistory } from 'vue-router';

import App from './App.vue';

import './assets/main.css';

// Simple router setup - Nuxt UI Button requires Vue Router
const routes: RouteRecordRaw[] = [{ component: { template: '<div />' }, path: '/' }];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

// oxlint-disable-next-line no-unsafe-argument -- Vue SFC import typing
const app = createApp(App);
const pinia = createPinia();

app.use(pinia);
app.use(PiniaColada);
app.use(router);
app.use(ui);
app.mount('#app');
