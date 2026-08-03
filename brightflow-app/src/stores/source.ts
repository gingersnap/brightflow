/**
 * The user's reporting-period selection.
 *
 * Client state only: the source list itself lives on the colada cache (see
 * `composables/useSources`). Period is persisted to localStorage because it is
 * a preference that should survive a reload.
 */

import { useLocalStorage } from '@vueuse/core';
import { defineStore } from 'pinia';

export const useSourceStore = defineStore('source', () => {
  const period = useLocalStorage('brightflow-period', '30d');

  function reset(): void {
    period.value = '30d';
  }

  return {
    period,
    reset,
  };
});
