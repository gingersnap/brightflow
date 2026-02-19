<script setup lang="ts">
import { onMounted } from 'vue';
import { RefreshCw } from 'lucide-vue-next';
import { useConnectStore } from '@/stores/connect';
import ConnectorCard from './ConnectorCard.vue';

const connectStore = useConnectStore();

onMounted(async () => {
  await Promise.all([connectStore.fetchConnectors(), connectStore.fetchRuns()]);
});

async function refresh(): Promise<void> {
  await Promise.all([connectStore.fetchConnectors(), connectStore.fetchRuns()]);
}

function handleRun(name: string): void {
  connectStore.runConnector(name);
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
        @click="refresh"
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
          Place YAML config files in the <code class="px-1 py-0.5 bg-elevated rounded">configs/</code> directory
        </p>
      </div>

      <!-- Connector grid -->
      <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <ConnectorCard
          v-for="connector in connectStore.connectors"
          :key="connector.name"
          :connector="connector"
          :running="connectStore.isRunning(connector.name)"
          :last-run="connectStore.latestRun(connector.name)"
          @run="handleRun(connector.name)"
        />
      </div>

      <!-- Run history -->
      <div v-if="connectStore.runs.length > 0" class="mt-8">
        <h3 class="text-sm font-semibold text-highlighted mb-3">Run History</h3>
        <div class="space-y-2">
          <div
            v-for="run in connectStore.runs"
            :key="run.id"
            class="flex items-center gap-3 px-3 py-2 rounded-lg border border-default bg-default text-sm"
          >
            <span
              class="h-2 w-2 rounded-full shrink-0"
              :class="{
                'bg-green-500': run.status === 'completed',
                'bg-red-500': run.status === 'failed',
                'bg-blue-500 animate-pulse': run.status === 'running',
                'bg-gray-400': run.status === 'pending',
              }"
            />
            <span class="font-medium text-highlighted">{{ run.connector }}</span>
            <span class="text-xs text-muted capitalize">{{ run.status }}</span>
            <span v-if="run.tables_ingested.length > 0" class="text-xs text-muted">
              {{ run.tables_ingested.length }} table{{ run.tables_ingested.length === 1 ? '' : 's' }}
            </span>
            <div class="flex-1" />
            <span class="text-xs text-muted">
              {{ new Date(run.started_at).toLocaleString() }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
