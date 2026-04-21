<script setup lang="ts">
import { useQuery, useQueryCache } from '@pinia/colada';
import { Calendar, Play } from 'lucide-vue-next';
import { computed, ref } from 'vue';

import NewRunDialog from '@/components/connect/NewRunDialog.vue';
import { connectApi } from '@/services/api';
import { useConnectStore } from '@/stores/connect';
import type { AvailableConnectorResponse } from '@/types';

const connectStore = useConnectStore();
const queryCache = useQueryCache();

const { data: available } = useQuery({
  key: ['connectors-available'],
  query: async () => {
    const result = await connectApi.listAvailable();
    return result ?? ([] as AvailableConnectorResponse[]);
  },
});

const availableList = computed(() => available.value ?? []);

type DialogMode = 'run' | 'schedule';
const dialogMode = ref<DialogMode>('run');

function openNewRun(): void {
  dialogMode.value = 'run';
  connectStore.showNewRunDialog = true;
}

function openNewSchedule(): void {
  dialogMode.value = 'schedule';
  connectStore.showNewRunDialog = true;
}

function onDialogDone(): void {
  queryCache.invalidateQueries({ key: ['connectors'] });
  queryCache.invalidateQueries({ key: ['connectors-available'] });
  queryCache.invalidateQueries({ key: ['sync-runs'] });
  queryCache.invalidateQueries({ key: ['unified-sources'] });
}
</script>

<template>
  <UDashboardPanel id="sources-new">
    <template #header>
      <UDashboardNavbar title="Add source">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="p-4">
        <div class="mx-auto max-w-3xl space-y-6">
          <div class="flex items-center justify-center gap-3">
            <UButton size="lg" @click="openNewRun">
              <Play class="mr-1.5 h-4 w-4" />
              New Run
            </UButton>
            <UButton size="lg" variant="outline" @click="openNewSchedule">
              <Calendar class="mr-1.5 h-4 w-4" />
              New Schedule
            </UButton>
          </div>
        </div>
      </div>

      <NewRunDialog
        :open="connectStore.showNewRunDialog"
        :available="availableList"
        :mode="dialogMode"
        @close="connectStore.showNewRunDialog = false"
        @done="onDialogDone"
      />
    </template>
  </UDashboardPanel>
</template>
