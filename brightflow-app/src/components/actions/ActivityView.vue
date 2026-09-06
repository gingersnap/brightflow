<script setup lang="ts">
/**
 * Activity page shell under Settings: the one place the curation audit
 * trail is shown. The feed is global — every action on every source, human
 * or agent — so this page takes no scope; the tools that dispatch actions
 * link here rather than embedding the feed themselves.
 */

import ActivityFeed from '@/components/actions/ActivityFeed.vue';
</script>

<template>
  <UDashboardPanel id="activity">
    <template #header>
      <UDashboardNavbar title="Activity">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="mx-auto h-full w-full max-w-3xl">
        <ActivityFeed />
      </div>
    </template>
  </UDashboardPanel>
</template>
