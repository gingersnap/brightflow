import { defineStore } from 'pinia';
import { ref } from 'vue';
import { insightsApi, type AnalysisTree } from '@/services/api';
import { useDatasetStore } from './dataset';

export type ReportType = 'review' | 'trends';
export type Cadence = 'daily' | 'weekly' | 'monthly';

export const useInsightsStore = defineStore('insights', () => {
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

  async function runReview(selectedCadence: Cadence = cadence.value): Promise<void> {
    const datasetStore = useDatasetStore();
    if (!datasetStore.id) return;

    loading.value = true;
    error.value = null;
    cadence.value = selectedCadence;
    reportType.value = 'review';

    try {
      const result = await insightsApi.runReview(datasetStore.id, selectedCadence);
      if (result) {
        tree.value = result.tree;
        executionTimeMs.value = result.executionTimeMs;
        nodeCount.value = result.nodeCount;
        findingCount.value = result.findingCount;
        firstLevelCount.value = result.firstLevelCount;
        deeperCount.value = result.deeperCount;
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Analysis failed';
      tree.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function runTrends(): Promise<void> {
    const datasetStore = useDatasetStore();
    if (!datasetStore.id) return;

    loading.value = true;
    error.value = null;
    reportType.value = 'trends';

    try {
      const result = await insightsApi.runTrends(datasetStore.id);
      if (result) {
        tree.value = result.tree;
        executionTimeMs.value = result.executionTimeMs;
        nodeCount.value = result.nodeCount;
        findingCount.value = result.findingCount;
        firstLevelCount.value = result.firstLevelCount;
        deeperCount.value = result.deeperCount;
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Analysis failed';
      tree.value = null;
    } finally {
      loading.value = false;
    }
  }

  function reset(): void {
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
    tree,
    reportType,
    cadence,
    loading,
    error,
    executionTimeMs,
    nodeCount,
    findingCount,
    firstLevelCount,
    deeperCount,
    runReview,
    runTrends,
    reset,
  };
});
