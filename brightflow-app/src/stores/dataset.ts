/**
 * The loaded dataset: its columns and its identity.
 *
 * Client state only — the load itself (fetch + initial rows) lives in the
 * explore tool; this store receives the response and resets UI state keyed to
 * the previous table's columns. This is not the whole reset — `resetAllStores`
 * covers the rest — so treat it as the minimum this store owes its own
 * consumers, not as a guarantee that nothing stale survives anywhere.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { ColumnInfo, LoadTableResponse } from '@/types';
import { isNumericDtype } from '@/utils/dtype';

import { useUiStore } from './ui';

export const useDatasetStore = defineStore('dataset', () => {
  // State - current dataset
  const id = ref('default');
  const name = ref<string | null>(null);
  const rowCount = ref<number | null>(null);
  const columns = ref<ColumnInfo[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  // Computed
  const numericColumns = computed(() => columns.value.filter((c) => isNumericDtype(c.dtype)));

  const hasData = computed(() => columns.value.length > 0);

  // Set state from a LoadTableResponse (REST-based, no WS needed)
  function setFromLoadResponse(response: LoadTableResponse): void {
    id.value = response.id;
    name.value = response.name;
    rowCount.value = response.rowCount;
    columns.value = response.columns;
    loading.value = false;
    error.value = null;

    // Reset UI state for new dataset
    const uiStore = useUiStore();
    uiStore.resetForNewDataset();
  }

  function reset(): void {
    id.value = 'default';
    name.value = null;
    rowCount.value = null;
    columns.value = [];
    error.value = null;
  }

  return {
    // Current dataset state
    id,
    name,
    rowCount,
    columns,
    loading,
    error,
    // Computed
    numericColumns,
    hasData,
    // Actions
    setFromLoadResponse,
    reset,
  };
});
