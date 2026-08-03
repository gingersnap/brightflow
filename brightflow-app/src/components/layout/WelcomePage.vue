<script setup lang="ts">
 /**
 * Static empty-state panel shown before any source is selected: brand
 * mark and a pointer to the sidebar.
 */
</script>

<template>
  <UDashboardPanel id="welcome">
    <template #header>
      <UDashboardNavbar title="Welcome">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="flex flex-1 items-center justify-center">
        <div class="text-center">
          <UIcon name="i-lucide-layers" class="mx-auto h-10 w-10 text-brand" />
          <h1 class="mt-3 text-lg font-semibold text-highlighted">Welcome to Brightflow</h1>
          <p class="mt-1 text-sm text-muted">Select a source from the sidebar to get started.</p>
        </div>
      </div>
    </template>
  </UDashboardPanel>
</template>
