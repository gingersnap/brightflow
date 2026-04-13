import { defineStore } from 'pinia';
import { ref } from 'vue';

import { type AnalysisTree, insightsApi } from '@/services/api';

export type ReportType = 'review' | 'trends';
export type Cadence = 'daily' | 'weekly' | 'monthly';

export const useInsightsStore = defineStore('insights', () => {
  const selectedTable = ref<string | null>(null);
  const tree = ref<AnalysisTree | null>(null);
  const reportType = ref<ReportType>('review');
  const cadence = ref<Cadence>('weekly');
  const loading = ref(false);
  const error = ref<string | null>(null);
  const executionTimeMs = ref<number | null>(null);
  const nodeCount = ref(0);
  const findingCount = ref(0);
  const firstLevelCount = ref(0);
  const deeperCount = ref(0);

  function selectTable(name: string): void {
    if (name === selectedTable.value) {
      return;
    }
    selectedTable.value = name;
    tree.value = null;
    error.value = null;
    executionTimeMs.value = null;
    nodeCount.value = 0;
    findingCount.value = 0;
    firstLevelCount.value = 0;
    deeperCount.value = 0;
  }

  async function runReview(selectedCadence: Cadence = cadence.value): Promise<void> {
    if (selectedTable.value == null) {
      error.value = 'No table selected';
      return;
    }

    loading.value = true;
    error.value = null;
    cadence.value = selectedCadence;
    reportType.value = 'review';

    try {
      const result = await insightsApi.runReview(selectedTable.value, selectedCadence);
      if (result) {
        // oxlint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- Backend-generated tree shape
        tree.value = result.tree as AnalysisTree;
        executionTimeMs.value = result.executionTimeMs;
        nodeCount.value = result.nodeCount;
        findingCount.value = result.findingCount;
        firstLevelCount.value = result.firstLevelCount;
        deeperCount.value = result.deeperCount;
      }
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Analysis failed';
      tree.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function runTrends(): Promise<void> {
    if (selectedTable.value == null) {
      error.value = 'No table selected';
      return;
    }

    loading.value = true;
    error.value = null;
    reportType.value = 'trends';

    try {
      const result = await insightsApi.runTrends(selectedTable.value);
      if (result) {
        // oxlint-disable-next-line @typescript-eslint/no-unsafe-type-assertion -- Backend-generated tree shape
        tree.value = result.tree as AnalysisTree;
        executionTimeMs.value = result.executionTimeMs;
        nodeCount.value = result.nodeCount;
        findingCount.value = result.findingCount;
        firstLevelCount.value = result.firstLevelCount;
        deeperCount.value = result.deeperCount;
      }
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Analysis failed';
      tree.value = null;
    } finally {
      loading.value = false;
    }
  }

  function reset(): void {
    selectedTable.value = null;
    tree.value = null;
    error.value = null;
    executionTimeMs.value = null;
    nodeCount.value = 0;
    findingCount.value = 0;
    firstLevelCount.value = 0;
    deeperCount.value = 0;
    loading.value = false;
  }

  return {
    cadence,
    deeperCount,
    error,
    executionTimeMs,
    findingCount,
    firstLevelCount,
    loading,
    nodeCount,
    reportType,
    reset,
    runReview,
    runTrends,
    selectTable,
    selectedTable,
    tree,
  };
});
