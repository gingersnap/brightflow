import router from '@/router';

import { useDatasetStore } from './dataset';
import { useInsightsStore } from './insights';
import { usePivotStore } from './pivot';
/**
 * Store utilities and exports
 */
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
export { useConnectStore } from './connect';
export { useAuthStore } from './auth';
export { useSourceStore } from './source';
