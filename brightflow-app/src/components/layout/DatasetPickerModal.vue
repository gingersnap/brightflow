<script setup lang="ts">
import { AlertCircle, Database, Table2, X } from '@lucide/vue';
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
    // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the component ref
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to fetch tables';
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
        <div class="mb-6 flex items-center gap-3">
          <div class="rounded-lg bg-primary-500/10 p-2">
            <Database class="h-6 w-6 text-primary-500" />
          </div>
          <div class="flex-1">
            <h2 class="text-lg font-semibold text-highlighted">Choose a Dataset</h2>
            <p class="text-sm text-muted">Select a table to load and explore</p>
          </div>
          <button
            v-if="!loading"
            class="rounded-md p-1 text-muted transition-colors hover:bg-muted/50 hover:text-default"
            @click="emit('close')"
          >
            <X class="h-5 w-5" />
          </button>
        </div>

        <!-- Fetching tables state -->
        <div v-if="fetching" class="flex items-center justify-center py-12">
          <UIcon name="i-lucide-loader-circle" class="size-6 animate-spin text-muted" />
          <span class="ml-2 text-muted">Loading available tables...</span>
        </div>

        <!-- Error state -->
        <div v-else-if="error" class="py-8 text-center">
          <AlertCircle class="mx-auto mb-3 h-8 w-8 text-red-500" />
          <p class="text-sm text-red-500">{{ error }}</p>
          <UButton variant="ghost" size="md" class="mt-4" @click="fetchTables"> Try again </UButton>
        </div>

        <!-- Empty state -->
        <div v-else-if="tables.length === 0" class="py-8 text-center">
          <Table2 class="mx-auto mb-3 h-8 w-8 text-muted" />
          <p class="text-sm text-muted">No tables available</p>
          <p class="mt-1 text-sm text-muted">Run a data sync to populate the data store</p>
        </div>

        <!-- Table list -->
        <div v-else class="max-h-80 space-y-2 overflow-y-auto">
          <button
            v-for="table in tables"
            :key="table.name"
            :disabled="loading"
            class="group w-full rounded-lg border p-4 text-left transition-colors"
            :class="
              loading && selectedTable === table.name
                ? 'border-primary-500/50 bg-elevated'
                : loading
                  ? 'cursor-not-allowed border-default opacity-50'
                  : 'border-default hover:border-primary-500/50 hover:bg-elevated'
            "
            @click="handleSelect(table)"
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <UIcon
                  v-if="loading && selectedTable === table.name"
                  name="i-lucide-loader-circle"
                  class="size-5 animate-spin text-primary-500"
                />
                <Table2
                  v-else
                  class="h-5 w-5 text-muted transition-colors group-hover:text-primary-500"
                />
                <div>
                  <div class="font-medium text-highlighted">{{ table.name }}</div>
                  <div class="mt-0.5 text-sm text-muted">
                    {{ formatRowCount(table.num_rows) }}
                    <span v-if="table.num_files" class="ml-2">
                      {{ table.num_files }} file{{ table.num_files !== 1 ? 's' : '' }}
                    </span>
                  </div>
                </div>
              </div>
              <div class="text-sm text-muted">
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
