/**
 * Cross-store reset.
 *
 * `resetAllStores` deliberately spares the source store: sources outlive a
 * dataset switch, while everything else is keyed to the previous table's
 * columns and would surface stale state under a new dataset's name. (Auth is
 * not a store — session state lives with `useAuth` and the backend cookie.)
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
 * Note: the source store is NOT reset here — it persists across dataset switches.
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
