<script setup lang="ts">
/**
 * Activity page shell under Settings, in two tabs. Logs is the audit trail:
 * every action by a person or an agent, and every background job once it
 * finished, in one order. Running jobs is what is happening now: syncs,
 * enrichment, agent runs and insight runs, with cancel where a kind allows
 * it. Both are global — every source — so this page takes no scope; the
 * tools that start work link here rather than embedding either list.
 */

import { computed, ref } from 'vue';

import ActivityFeed from '@/components/actions/ActivityFeed.vue';
import JobsPanel from '@/components/actions/JobsPanel.vue';

type Tab = 'logs' | 'jobs';
const tab = ref<Tab>('logs');
const tabs = computed(() => [
  { icon: 'i-lucide-history', label: 'Logs', value: 'logs' },
  { icon: 'i-lucide-loader-circle', label: 'Running jobs', value: 'jobs' },
]);
</script>

<template>
  <UDashboardPanel id="activity">
    <template #header>
      <UDashboardNavbar title="Activity">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
        <template #right>
          <UTabs
            :model-value="tab"
            :items="tabs"
            :content="false"
            size="md"
            @update:model-value="(v) => (tab = String(v) === 'jobs' ? 'jobs' : 'logs')"
          />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="mx-auto h-full w-full max-w-3xl">
        <ActivityFeed v-if="tab === 'logs'" />
        <JobsPanel v-else />
      </div>
    </template>
  </UDashboardPanel>
</template>
