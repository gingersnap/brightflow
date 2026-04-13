<script setup lang="ts">
import { onMounted } from 'vue';

import LoginPage from './components/auth/LoginPage.vue';
import AppSidebar from './components/layout/AppSidebar.vue';
import { resetOnLogout } from './stores';
import { useAuthStore } from './stores/auth';
import { useConnectionStore } from './stores/connection';
import { useSystemStore } from './stores/system';

const authStore = useAuthStore();
const connectionStore = useConnectionStore();
const systemStore = useSystemStore();

onMounted(() => {
  authStore.checkAuth();
});

async function handleLogout(): Promise<void> {
  connectionStore.disconnect();
  systemStore.disconnect();
  resetOnLogout();
  await authStore.logout();
}
</script>

<template>
  <UApp>
    <!-- Auth loading state -->
    <div v-if="authStore.loading" class="flex h-screen items-center justify-center bg-default">
      <p class="text-muted">Loading...</p>
    </div>

    <!-- Login page -->
    <div v-else-if="!authStore.isAuthenticated" class="flex h-screen flex-col bg-default">
      <LoginPage />
    </div>

    <!-- Main app (authenticated) -->
    <UDashboardGroup
      v-else
      unit="rem"
      storage="local"
      storage-key="brightflow-dashboard"
      class="h-screen bg-default"
    >
      <AppSidebar @logout="handleLogout" />
      <router-view />
    </UDashboardGroup>
  </UApp>
</template>
