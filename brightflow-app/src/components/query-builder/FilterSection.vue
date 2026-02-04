<script setup lang="ts">
import { Plus } from 'lucide-vue-next';
import { useQueryStore } from '@/stores/query';
import FilterRow from './FilterRow.vue';

const queryStore = useQueryStore();
</script>

<template>
  <div class="pt-3 space-y-2">
    <!-- Filter rows -->
    <FilterRow
      v-for="filter in queryStore.filters"
      :key="filter.id"
      :filter="filter"
      @update="(updates) => queryStore.updateFilter(filter.id, updates)"
      @remove="queryStore.removeFilter(filter.id)"
    />

    <!-- Empty state -->
    <div v-if="queryStore.filters.length === 0" class="text-sm text-muted py-2">
      No filters. Click "Add Filter" to filter your data.
    </div>

    <!-- Add filter button -->
    <UButton
      variant="ghost"
      color="neutral"
      size="sm"
      @click="queryStore.addFilter"
    >
      <Plus class="w-4 h-4 mr-1" />
      Add Filter
    </UButton>
  </div>
</template>
