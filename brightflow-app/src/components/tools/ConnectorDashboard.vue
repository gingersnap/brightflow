<script setup lang="ts">
/**
 * Landing view for a connector-backed source: its synced tables, each with a
 * jump into Explore, and a coarse ready / waiting-for-first-sync status.
 * Read-only — the only action here is navigation.
 */

import { Cable, Search, Table2 } from '@lucide/vue';
import { useRouter } from 'vue-router';

import type { UnifiedSource } from '@/types';

const props = defineProps<{
  source: UnifiedSource;
  sourceId: string;
}>();

const router = useRouter();

function openInExplore(tableName: string): void {
  router.push({ name: 'explore-table', params: { sourceId: props.sourceId, table: tableName } });
}
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-4">
    <div class="mb-4 flex items-center gap-3">
      <Cable class="h-5 w-5 text-muted" />
      <div>
        <h2 class="text-sm font-semibold text-highlighted">{{ source.name }}</h2>
        <p class="text-sm text-muted">{{ source.connectorName }} connector</p>
      </div>
    </div>

    <!-- Tables -->
    <div class="mb-4">
      <h3 class="mb-3 text-sm font-medium text-highlighted">Tables</h3>
      <div v-if="source.tables.length > 0" class="space-y-2">
        <div
          v-for="table in source.tables"
          :key="table.name"
          class="flex items-center justify-between rounded-lg border border-default bg-elevated p-3"
        >
          <div class="flex items-center gap-2">
            <Table2 class="h-4 w-4 text-muted" />
            <span class="text-sm font-medium text-highlighted">{{ table.name }}</span>
            <span v-if="table.numRows != null" class="text-sm text-muted">
              {{ table.numRows.toLocaleString() }} rows
            </span>
          </div>
          <UButton size="md" variant="ghost" @click="openInExplore(table.name)">
            <Search class="h-3.5 w-3.5" />
            Explore
          </UButton>
        </div>
      </div>
      <p v-else class="text-sm text-muted">No tables synced yet. Run a sync to populate data.</p>
    </div>

    <!-- Status -->
    <div class="rounded-lg border border-default bg-elevated p-4">
      <h3 class="mb-2 text-sm font-medium text-highlighted">Status</h3>
      <p class="text-sm text-muted">
        <template v-if="source.ready"> Data is synced and ready for exploration. </template>
        <template v-else> Waiting for first sync to complete. </template>
      </p>
    </div>
  </div>
</template>
