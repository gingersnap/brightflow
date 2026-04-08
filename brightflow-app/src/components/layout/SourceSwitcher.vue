<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { ChevronDown } from 'lucide-vue-next';
import { ref } from 'vue';

import { sourceApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';
import type { UnifiedSource } from '@/types';

const sourceStore = useSourceStore();
const open = ref(false);

const { data: sources } = useQuery({
  key: ['unified-sources'],
  query: async () => {
    const result = await sourceApi.unifiedList();
    const data = result ?? ([] as UnifiedSource[]);
    sourceStore.setSourcesData(data);
    return data;
  },
});

function pickSource(id: string): void {
  sourceStore.selectSource(id);
  open.value = false;
}

function goToAll(): void {
  sourceStore.clearSource();
  open.value = false;
}
</script>

<template>
  <div class="relative">
    <button
      class="flex cursor-pointer items-center gap-2 rounded-lg border border-default px-3 py-1.5 transition-colors hover:border-primary-500/50 hover:bg-elevated"
      @click="open = !open"
    >
      <span class="text-sm font-medium text-highlighted">
        {{ sourceStore.selectedSource?.name ?? 'Select source' }}
      </span>
      <ChevronDown class="h-4 w-4 text-muted" />
    </button>

    <!-- Dropdown -->
    <div
      v-if="open"
      class="absolute top-full left-0 z-50 mt-1 w-56 rounded-lg border border-default bg-default shadow-lg"
    >
      <div class="max-h-64 overflow-y-auto p-1">
        <button
          v-for="source in sources"
          :key="source.id"
          class="flex w-full cursor-pointer items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-elevated"
          :class="
            source.id === sourceStore.selectedSourceId
              ? 'font-medium text-primary-500'
              : 'text-highlighted'
          "
          @click="pickSource(source.id)"
        >
          {{ source.name }}
        </button>
      </div>
      <div class="border-t border-default p-1">
        <button
          class="flex w-full cursor-pointer items-center rounded-md px-3 py-2 text-left text-sm text-muted transition-colors hover:bg-elevated hover:text-highlighted"
          @click="goToAll"
        >
          All Sources
        </button>
      </div>
    </div>
  </div>
</template>
