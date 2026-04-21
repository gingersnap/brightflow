<script setup lang="ts">
import { useQueryCache } from '@pinia/colada';
import { RefreshCw } from 'lucide-vue-next';

import SchedulesPanel from '@/components/connect/SchedulesPanel.vue';

const queryCache = useQueryCache();

function refreshAll(): void {
  queryCache.invalidateQueries({ key: ['connectors'] });
  queryCache.invalidateQueries({ key: ['sync-runs'] });
}
</script>

<template>
  <UDashboardPanel id="schedules">
    <template #header>
      <UDashboardNavbar title="Schedules">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
        <template #trailing>
          <UButton variant="ghost" size="sm" @click="refreshAll">
            <RefreshCw class="mr-1.5 h-3.5 w-3.5" />
            Refresh
          </UButton>
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="p-4">
        <div class="mx-auto max-w-3xl">
          <SchedulesPanel />
        </div>
      </div>
    </template>
  </UDashboardPanel>
</template>
