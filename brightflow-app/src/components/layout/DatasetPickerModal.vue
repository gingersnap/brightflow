<script setup lang="ts">
import { AlertCircle, Database, Loader2, Table2, X } from 'lucide-vue-next';
import { ref, watch } from 'vue';

import { type TableInfo, tableApi } from '@/services/api';

const props = defineProps<{
  open: boolean;
  loading: boolean;
}>();

const selectedTable = ref<string | null>(null);

const emit = defineEmits<{
  select: [table: TableInfo];
  close: [];
}>();

const tables = ref<TableInfo[]>([]);
const fetching = ref(false);
const error = ref<string | null>(null);

async function fetchTables(): Promise<void> {
  fetching.value = true;
  error.value = null;

  try {
    const result = await tableApi.listAvailable();
    tables.value = result ?? [];
  } catch (error) {
    error.value = error instanceof Error ? error.message : 'Failed to fetch tables';
  } finally {
    fetching.value = false;
  }
}

function handleSelect(table: TableInfo): void {
  if (props.loading) {
    return;
  }
  selectedTable.value = table.name;
  emit('select', table);
}

function formatRowCount(count: number | null | undefined): string {
  if (count === null || count === undefined) {
    return 'Unknown rows';
  }
  return `${count.toLocaleString()} rows`;
}

// Fetch tables when modal opens, reset selection
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      selectedTable.value = null;
      if (tables.value.length === 0 && !fetching.value) {
        fetchTables();
      }
    }
  },
  { immediate: true },
);
</script>

<template>
  <UModal
    :open="open"
    :dismissible="!loading"
    :ui="{ overlay: 'z-50', content: 'z-50' }"
    class="w-full max-w-lg"
    @update:open="
      (val: boolean) => {
        if (!val) emit('close');
      }
    "
  >
    <template #content>
      <div class="p-6">
        <div class="flex items-center gap-3 mb-6">
          <div class="p-2 rounded-lg bg-primary-500/10">
            <Database class="w-6 h-6 text-primary-500" />
          </div>
          <div class="flex-1">
            <h2 class="text-lg font-semibold text-highlighted">Choose a Dataset</h2>
            <p class="text-sm text-muted">Select a table to load and explore</p>
          </div>
          <button
            v-if="!loading"
            class="p-1 rounded-md text-muted hover:text-default hover:bg-muted/50 transition-colors"
            @click="emit('close')"
          >
            <X class="w-5 h-5" />
          </button>
        </div>

        <!-- Fetching tables state -->
        <div v-if="fetching" class="flex items-center justify-center py-12">
          <Loader2 class="w-6 h-6 animate-spin text-muted" />
          <span class="ml-2 text-muted">Loading available tables...</span>
        </div>

        <!-- Error state -->
        <div v-else-if="error" class="py-8 text-center">
          <AlertCircle class="w-8 h-8 mx-auto mb-3 text-red-500" />
          <p class="text-sm text-red-500">{{ error }}</p>
          <UButton variant="ghost" size="sm" class="mt-4" @click="fetchTables"> Try again </UButton>
        </div>

        <!-- Empty state -->
        <div v-else-if="tables.length === 0" class="py-8 text-center">
          <Table2 class="w-8 h-8 mx-auto mb-3 text-muted" />
          <p class="text-sm text-muted">No tables available</p>
          <p class="text-xs text-muted mt-1">Run a data sync to populate the data store</p>
        </div>

        <!-- Table list -->
        <div v-else class="space-y-2 max-h-80 overflow-y-auto">
          <button
            v-for="table in tables"
            :key="table.name"
            :disabled="loading"
            class="w-full p-4 text-left rounded-lg border transition-colors group"
            :class="
              loading && selectedTable === table.name
                ? 'border-primary-500/50 bg-elevated'
                : loading
                  ? 'border-default opacity-50 cursor-not-allowed'
                  : 'border-default hover:border-primary-500/50 hover:bg-elevated'
            "
            @click="handleSelect(table)"
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <Loader2
                  v-if="loading && selectedTable === table.name"
                  class="w-5 h-5 text-primary-500 animate-spin"
                />
                <Table2
                  v-else
                  class="w-5 h-5 text-muted group-hover:text-primary-500 transition-colors"
                />
                <div>
                  <div class="font-medium text-highlighted">{{ table.name }}</div>
                  <div class="text-xs text-muted mt-0.5">
                    {{ formatRowCount(table.num_rows) }}
                    <span v-if="table.num_files" class="ml-2">
                      {{ table.num_files }} file{{ table.num_files !== 1 ? 's' : '' }}
                    </span>
                  </div>
                </div>
              </div>
              <div class="text-xs text-muted">
                <span v-if="loading && selectedTable === table.name">Loading...</span>
                <span v-else>v{{ table.version }}</span>
              </div>
            </div>
          </button>
        </div>
      </div>
    </template>
  </UModal>
</template>
