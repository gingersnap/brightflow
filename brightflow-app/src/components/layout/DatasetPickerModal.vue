<script setup lang="ts">
import { ref, watch } from 'vue';
import { Database, Loader2, AlertCircle, Table2 } from 'lucide-vue-next';
import { tableApi, type TableInfo } from '@/services/api';

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  select: [table: TableInfo];
}>();

const tables = ref<TableInfo[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);

async function fetchTables(): Promise<void> {
  loading.value = true;
  error.value = null;

  try {
    const result = await tableApi.listAvailable();
    tables.value = result ?? [];
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch tables';
  } finally {
    loading.value = false;
  }
}

function handleSelect(table: TableInfo): void {
  emit('select', table);
}

function formatRowCount(count: number | null | undefined): string {
  if (count === null || count === undefined) return 'Unknown rows';
  return `${count.toLocaleString()} rows`;
}

// Fetch tables when modal opens
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen && tables.value.length === 0 && !loading.value) {
      fetchTables();
    }
  },
  { immediate: true },
);
</script>

<template>
  <UModal
    :open="open"
    :close-button="false"
    :dismissible="false"
    class="w-full max-w-lg"
  >
    <template #content>
      <div class="p-6">
        <div class="flex items-center gap-3 mb-6">
          <div class="p-2 rounded-lg bg-primary-500/10">
            <Database class="w-6 h-6 text-primary-500" />
          </div>
          <div>
            <h2 class="text-lg font-semibold text-highlighted">Choose a Dataset</h2>
            <p class="text-sm text-muted">Select a table to load and explore</p>
          </div>
        </div>

        <!-- Loading state -->
        <div v-if="loading" class="flex items-center justify-center py-12">
          <Loader2 class="w-6 h-6 animate-spin text-muted" />
          <span class="ml-2 text-muted">Loading available tables...</span>
        </div>

        <!-- Error state -->
        <div v-else-if="error" class="py-8 text-center">
          <AlertCircle class="w-8 h-8 mx-auto mb-3 text-red-500" />
          <p class="text-sm text-red-500">{{ error }}</p>
          <UButton
            variant="ghost"
            size="sm"
            class="mt-4"
            @click="fetchTables"
          >
            Try again
          </UButton>
        </div>

        <!-- Empty state -->
        <div v-else-if="tables.length === 0" class="py-8 text-center">
          <Table2 class="w-8 h-8 mx-auto mb-3 text-muted" />
          <p class="text-sm text-muted">No tables available</p>
          <p class="text-xs text-muted mt-1">
            Run a data sync to populate the Delta store
          </p>
        </div>

        <!-- Table list -->
        <div v-else class="space-y-2 max-h-80 overflow-y-auto">
          <button
            v-for="table in tables"
            :key="table.name"
            class="w-full p-4 text-left rounded-lg border border-default hover:border-primary-500/50 hover:bg-elevated transition-colors group"
            @click="handleSelect(table)"
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <Table2 class="w-5 h-5 text-muted group-hover:text-primary-500 transition-colors" />
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
                v{{ table.version }}
              </div>
            </div>
          </button>
        </div>
      </div>
    </template>
  </UModal>
</template>
