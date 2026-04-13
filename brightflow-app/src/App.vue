<script setup lang="ts">
import { onMounted, watch } from 'vue';

import LoginPage from './components/auth/LoginPage.vue';
import ConnectView from './components/connect/ConnectView.vue';
import AppSidebar from './components/layout/AppSidebar.vue';
import SourceLanding from './components/layout/SourceLanding.vue';
import SourceLayout from './components/layout/SourceLayout.vue';
import SystemView from './components/system/SystemView.vue';
import { resetOnLogout } from './stores';
import { useAuthStore } from './stores/auth';
import { useConnectionStore } from './stores/connection';
import { useSourceStore } from './stores/source';
import { useSystemStore } from './stores/system';
import { useUiStore } from './stores/ui';

const authStore = useAuthStore();
const connectionStore = useConnectionStore();
const uiStore = useUiStore();
const systemStore = useSystemStore();
const sourceStore = useSourceStore();

onMounted(() => {
  authStore.checkAuth();
});

// Connect/disconnect system WS when toggling system view
watch(
  () => uiStore.showSystem,
  (show) => {
    if (show) {
      systemStore.connect();
    } else {
      systemStore.disconnect();
    }
  },
);

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

      <!-- Each view owns its own UDashboardPanel -->
      <SystemView v-if="uiStore.showSystem" />
      <ConnectView v-else-if="uiStore.showConnect" />
      <SourceLayout v-else-if="sourceStore.selectedSource" />
      <SourceLanding v-else />
    </UDashboardGroup>
  </UApp>
</template>
