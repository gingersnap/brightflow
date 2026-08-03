/**
 * The currently selected source and reporting period.
 *
 * Period is persisted to localStorage because it is a preference that should
 * survive a reload; the source list itself is pushed in by the component layer
 * rather than fetched here.
 */

import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import type { UnifiedSource } from '@/types';

export const useSourceStore = defineStore('source', () => {
  const storedPeriod = localStorage.getItem('brightflow-period');
  const period = ref(storedPeriod ?? '30d');

  // The actual source objects — set externally by component layer via setSourcesData
  const sourcesData = ref<UnifiedSource[]>([]);

  // Computed
  const availableTools = computed(() => {
    if (sourcesData.value.length === 0) {
      return [];
    }
    return [];
  });

  // Persist
  watch(period, (val) => {
    localStorage.setItem('brightflow-period', val);
  });

  // Actions
  function setSourcesData(sources: UnifiedSource[]): void {
    sourcesData.value = sources;
  }

  function getSourceById(id: string): UnifiedSource | undefined {
    return sourcesData.value.find((s) => s.id === id);
  }

  function reset(): void {
    period.value = '30d';
    sourcesData.value = [];
    localStorage.removeItem('brightflow-period');
  }

  return {
    availableTools,
    getSourceById,
    period,
    reset,
    setSourcesData,
    sourcesData,
  };
});
