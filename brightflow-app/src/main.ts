import ui from '@nuxt/ui/vue-plugin';
import { PiniaColada } from '@pinia/colada';
import { createPinia } from 'pinia';
import { createApp } from 'vue';

import App from './App.vue';
import router from './router';

import './assets/main.css';

// oxlint-disable-next-line no-unsafe-argument -- Vue SFC import typing
const app = createApp(App);
const pinia = createPinia();

app.use(pinia);
app.use(PiniaColada);
app.use(router);
app.use(ui);
app.mount('#app');
