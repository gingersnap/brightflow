/**
 * Store barrel and the cross-store reset.
 *
 * `resetAllStores` deliberately spares auth and source: those outlive a dataset
 * switch, while everything else is keyed to the previous table's columns and
 * would surface stale state under a new dataset's name.
 */

import router from '@/router';

import { useDatasetStore } from './dataset';
import { useInsightsStore } from './insights';
import { usePivotStore } from './pivot';
import { useQueryStore } from './query';
import { useResultsStore } from './results';
import { useSourceStore } from './source';
import { useUiStore } from './ui';

/**
 * Reset all stores to initial state.
 * Used when switching datasets to ensure clean slate.
 * Note: Auth store and Source store are NOT reset here — they persist across dataset switches.
 */
export function resetAllStores(): void {
  const queryStore = useQueryStore();
  const resultsStore = useResultsStore();
  const pivotStore = usePivotStore();
  const datasetStore = useDatasetStore();
  const uiStore = useUiStore();
  const insightsStore = useInsightsStore();

  queryStore.reset();
  resultsStore.clear();
  pivotStore.reset();
  datasetStore.reset();
  insightsStore.reset();
  uiStore.resetForNewDataset();
}

/**
 * Full reset for logout — includes source store.
 */
export function resetOnLogout(): void {
  resetAllStores();
  const sourceStore = useSourceStore();
  sourceStore.reset();
  void router.push('/');
}

// Re-export stores for convenience
export { useQueryStore } from './query';
export { useResultsStore } from './results';
export { usePivotStore } from './pivot';
export { useDatasetStore } from './dataset';
export { useUiStore } from './ui';
export { useConnectionStore } from './connection';
export { useInsightsStore } from './insights';
export { useSourceStore } from './source';
