<script setup lang="ts">
import { onMounted, watch } from 'vue';

import LoginPage from './components/auth/LoginPage.vue';
import ConnectView from './components/connect/ConnectView.vue';
import AppHeader from './components/layout/AppHeader.vue';
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
    <template v-else>
      <div class="flex h-screen flex-col bg-default">
        <AppHeader @logout="handleLogout" />

        <!-- System mode -->
        <template v-if="uiStore.showSystem">
          <div class="relative min-h-0 flex-1 overflow-hidden">
            <SystemView />
          </div>
        </template>

        <!-- Connect mode -->
        <template v-else-if="uiStore.showConnect">
          <div class="min-h-0 flex-1 overflow-hidden">
            <ConnectView />
          </div>
        </template>

        <!-- Source selected — show sidebar + tools -->
        <template v-else-if="sourceStore.selectedSource">
          <div class="min-h-0 flex-1 overflow-hidden">
            <SourceLayout />
          </div>
        </template>

        <!-- No source selected — show landing page -->
        <template v-else>
          <SourceLanding />
        </template>
      </div>
    </template>
  </UApp>
</template>
