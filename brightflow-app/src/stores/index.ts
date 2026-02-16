/**
 * Store utilities and exports
 */
import { useQueryStore } from './query';
import { useResultsStore } from './results';
import { usePivotStore } from './pivot';
import { useDatasetStore } from './dataset';
import { useUiStore } from './ui';

/**
 * Reset all stores to initial state.
 * Used when switching datasets to ensure clean slate.
 */
export function resetAllStores(): void {
  const queryStore = useQueryStore();
  const resultsStore = useResultsStore();
  const pivotStore = usePivotStore();
  const datasetStore = useDatasetStore();
  const uiStore = useUiStore();

  queryStore.reset();
  resultsStore.clear();
  pivotStore.reset();
  datasetStore.reset();
  uiStore.resetForNewDataset();
}

// Re-export stores for convenience
export { useQueryStore } from './query';
export { useResultsStore } from './results';
export { usePivotStore } from './pivot';
export { useDatasetStore } from './dataset';
export { useUiStore } from './ui';
export { useConnectionStore } from './connection';
