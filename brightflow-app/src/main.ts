import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import ui from '@nuxt/ui/vue-plugin'
import App from './App.vue'
import './assets/main.css'

// Simple router setup - Nuxt UI Button requires Vue Router
const routes: RouteRecordRaw[] = [
  { path: '/', component: { template: '<div />' } }
]

const router = createRouter({
  history: createWebHistory(),
  routes
})

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)
app.use(router)
app.use(ui)
app.mount('#app')
