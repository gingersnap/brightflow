/**
 * The currently selected source and reporting period.
 *
 * Period is persisted to localStorage because it is a preference that should
 * survive a reload; the source list itself is pushed in by the component layer
 * rather than fetched here.
 */

import { useLocalStorage } from '@vueuse/core';
import { defineStore } from 'pinia';
import { ref } from 'vue';

import type { UnifiedSource } from '@/types';

export const useSourceStore = defineStore('source', () => {
  const period = useLocalStorage('brightflow-period', '30d');

  // The actual source objects — set externally by component layer via setSourcesData
  const sourcesData = ref<UnifiedSource[]>([]);

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
  }

  return {
    getSourceById,
    period,
    reset,
    setSourcesData,
    sourcesData,
  };
});
