import { useDatasetStore } from './dataset';
import { useInsightsStore } from './insights';
import { usePivotStore } from './pivot';
/**
 * Store utilities and exports
 */
import { useQueryStore } from './query';
import { useResultsStore } from './results';
import { useUiStore } from './ui';

/**
 * Reset all stores to initial state.
 * Used when switching datasets to ensure clean slate.
 * Note: Auth store is NOT reset here — it persists across dataset switches.
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
