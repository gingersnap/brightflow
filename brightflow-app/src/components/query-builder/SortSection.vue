<script setup lang="ts">
import { ArrowUp, ArrowDown } from 'lucide-vue-next';
import { computed } from 'vue';
import { useQueryStore } from '@/stores/query';
import { useDatasetStore } from '@/stores/dataset';

const queryStore = useQueryStore();
const datasetStore = useDatasetStore();

const columnItems = computed(() => datasetStore.columns.map((col) => col.name));
</script>

<template>
  <div class="pt-3 space-y-3">
    <!-- Column select -->
    <div>
      <label class="text-xs font-medium text-muted mb-2 block">Sort By</label>
      <USelectMenu
        :model-value="queryStore.sortBy ?? ''"
        :items="columnItems"
        placeholder="Select column"
        @update:model-value="(val: string) => queryStore.sortBy = val"
      />
    </div>

    <!-- Direction toggle -->
    <div v-if="queryStore.sortBy" class="flex items-center gap-4">
      <label class="text-xs font-medium text-muted">Direction</label>
      <div class="flex gap-1">
        <UButton
          :variant="!queryStore.sortDescending ? 'solid' : 'outline'"
          :color="!queryStore.sortDescending ? 'primary' : 'neutral'"
          size="xs"
          @click="queryStore.sortDescending = false"
        >
          <ArrowUp class="w-3 h-3 mr-1" />
          Ascending
        </UButton>
        <UButton
          :variant="queryStore.sortDescending ? 'solid' : 'outline'"
          :color="queryStore.sortDescending ? 'primary' : 'neutral'"
          size="xs"
          @click="queryStore.sortDescending = true"
        >
          <ArrowDown class="w-3 h-3 mr-1" />
          Descending
        </UButton>
      </div>
    </div>
  </div>
</template>
