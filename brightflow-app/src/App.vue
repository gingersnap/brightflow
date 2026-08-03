<script setup lang="ts">
/**
 * Root shell: branches on session state between the loading screen, the
 * login page, and the authenticated dashboard group. Logout disconnects
 * both WebSocket-backed stores and resets client store state before
 * ending the session.
 */

import LoginPage from './components/auth/LoginPage.vue';
import CommandPalette from './components/command/CommandPalette.vue';
import AppSidebar from './components/layout/AppSidebar.vue';
import { useAuth } from './composables/useAuth';
import { resetOnLogout } from './stores';
import { useConnectionStore } from './stores/connection';
import { useSystemStore } from './stores/system';

// The session query fires on setup — no explicit checkAuth call needed.
const auth = useAuth();
const connectionStore = useConnectionStore();
const systemStore = useSystemStore();

async function handleLogout(): Promise<void> {
  connectionStore.disconnect();
  systemStore.disconnect();
  resetOnLogout();
  await auth.logout();
}
</script>

<template>
  <UApp>
    <!-- Auth loading state -->
    <div v-if="auth.loading.value" class="flex h-screen items-center justify-center bg-default">
      <p class="text-muted">Loading...</p>
    </div>

    <!-- Login page -->
    <div v-else-if="!auth.isAuthenticated.value" class="flex h-screen flex-col bg-default">
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
      <!-- Inside the authenticated branch so it is inert pre-auth. -->
      <CommandPalette />
    </UDashboardGroup>
  </UApp>
</template>
