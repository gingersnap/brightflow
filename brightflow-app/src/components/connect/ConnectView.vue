<script setup lang="ts">
import { onMounted } from 'vue';
import { RefreshCw } from 'lucide-vue-next';
import { useConnectStore } from '@/stores/connect';
import ConnectorCard from './ConnectorCard.vue';

const connectStore = useConnectStore();

onMounted(async () => {
  await connectStore.fetchConnectors();
});

function handleRun(name: string): void {
  connectStore.syncNow(name);
}

async function handleSchedule(name: string, intervalSecs: number): Promise<void> {
  await connectStore.updateSchedule(name, intervalSecs);
}

function handleUpdateToken(name: string, token: string): void {
  connectStore.updateToken(name, token);
}
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="flex items-center gap-3 px-4 py-2.5 border-b border-default bg-default">
      <h2 class="text-sm font-semibold text-highlighted">Data Connectors</h2>

      <div class="flex-1" />

      <UButton
        variant="ghost"
        size="sm"
        :loading="connectStore.loading"
        @click="connectStore.fetchConnectors()"
      >
        <RefreshCw class="w-3.5 h-3.5 mr-1.5" />
        Refresh
      </UButton>
    </div>

    <!-- Content -->
    <div class="flex-1 min-h-0 overflow-y-auto p-4">
      <!-- Error -->
      <div v-if="connectStore.error" class="mb-4 p-3 rounded-lg bg-red-500/10 text-red-500 text-sm">
        {{ connectStore.error }}
      </div>

      <!-- Empty state -->
      <div
        v-if="!connectStore.loading && connectStore.connectors.length === 0"
        class="flex flex-col items-center justify-center py-16 text-center"
      >
        <p class="text-muted mb-2">No connector configs found</p>
        <p class="text-xs text-muted">
          Place TOML config files in the <code class="px-1 py-0.5 bg-elevated rounded">configs/</code> directory
        </p>
      </div>

      <!-- Connector cards -->
      <div v-else class="space-y-3 max-w-3xl">
        <ConnectorCard
          v-for="connector in connectStore.connectors"
          :key="connector.name"
          :connector="connector"
          :running="connectStore.isRunning(connector.name)"
          :expanded="connectStore.expandedConnector === connector.name"
          :history="connectStore.runHistory.get(connector.name) ?? []"
          @run="handleRun(connector.name)"
          @schedule="handleSchedule(connector.name, $event)"
          @update-token="handleUpdateToken(connector.name, $event)"
          @toggle-history="connectStore.toggleHistory(connector.name)"
        />
      </div>
    </div>
  </div>
</template>
