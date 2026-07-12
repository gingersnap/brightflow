import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { type AnalysisNode, type AnalysisTree, insightsApi } from '@/services/api';

export type ReportType = 'review' | 'trends';
export type Cadence = 'daily' | 'weekly' | 'monthly';

/** UI filter state — applied client-side over the ranked findings */
export interface InsightFilters {
  /** Empty = all types pass */
  types: Set<string>;
  direction: 'up' | 'down' | 'both';
  /** Null = all measures pass */
  measure: string | null;
  minScore: number;
}

export const useInsightsStore = defineStore('insights', () => {
  const selectedSourceId = ref<string | null>(null);
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
  const totalCandidates = ref(0);
  const showAll = ref(false);

  const filters = ref<InsightFilters>({
    direction: 'both',
    measure: null,
    minScore: 0,
    types: new Set(),
  });

  // Returns root nodes filtered + sorted by significance
  const visibleRoots = computed<AnalysisNode[]>(() => {
    if (!tree.value) {
      return [];
    }
    const nodeMap = new Map<number, AnalysisNode>();
    for (const n of tree.value.nodes) {
      nodeMap.set(n.id, n);
    }
    const roots = tree.value.roots
      .map((id) => nodeMap.get(id))
      .filter((n): n is AnalysisNode => n !== undefined);

    return roots
      .filter((n) => {
        if (filters.value.types.size > 0 && !filters.value.types.has(n.analysis.type)) {
          return false;
        }
        if (n.significance < filters.value.minScore) {
          return false;
        }
        if (filters.value.direction !== 'both') {
          const dir = nodeDirection(n);
          if (dir !== null && dir !== filters.value.direction) {
            return false;
          }
        }
        if (filters.value.measure !== null) {
          const col = nodeMeasure(n);
          if (col !== null && col !== filters.value.measure) {
            return false;
          }
        }
        return true;
      })
      .toSorted((a, b) => {
        // Diversity-selected rank from the engine wins; fall back to score
        if (a.rank != null && b.rank != null) {
          return a.rank - b.rank;
        }
        if (a.rank != null) {
          return -1;
        }
        if (b.rank != null) {
          return 1;
        }
        return b.significance - a.significance;
      });
  });

  // The set of measures referenced anywhere in the tree (for the measure filter dropdown)
  const availableMeasures = computed<string[]>(() => {
    if (!tree.value) {
      return [];
    }
    const set = new Set<string>();
    for (const n of tree.value.nodes) {
      const col = nodeMeasure(n);
      if (col !== null) {
        set.add(col);
      }
    }
    return [...set].toSorted();
  });

  // The set of analysis types present in the roots (for filter chips)
  const availableTypes = computed<string[]>(() => {
    if (!tree.value) {
      return [];
    }
    const set = new Set<string>();
    for (const id of tree.value.roots) {
      const n = tree.value.nodes.find((x) => x.id === id);
      if (n) {
        set.add(n.analysis.type);
      }
    }
    return [...set].toSorted();
  });

  function selectTable(sourceId: string, name: string): void {
    if (sourceId === selectedSourceId.value && name === selectedTable.value) {
      return;
    }
    selectedSourceId.value = sourceId;
    selectedTable.value = name;
    resetState();
  }

  function resetState(): void {
    tree.value = null;
    error.value = null;
    executionTimeMs.value = null;
    nodeCount.value = 0;
    findingCount.value = 0;
    firstLevelCount.value = 0;
    deeperCount.value = 0;
    totalCandidates.value = 0;
    showAll.value = false;
    filters.value = { direction: 'both', measure: null, minScore: 0, types: new Set() };
  }

  async function runReview(selectedCadence: Cadence = cadence.value): Promise<void> {
    if (selectedSourceId.value == null || selectedTable.value == null) {
      error.value = 'No table selected';
      return;
    }
    loading.value = true;
    error.value = null;
    cadence.value = selectedCadence;
    reportType.value = 'review';
    try {
      const result = await insightsApi.runReview({
        cadence: selectedCadence,
        datasetId: selectedTable.value,
        sourceId: selectedSourceId.value,
      });
      handleResult(result);
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Analysis failed';
      tree.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function runTrends(): Promise<void> {
    if (selectedSourceId.value == null || selectedTable.value == null) {
      error.value = 'No table selected';
      return;
    }
    loading.value = true;
    error.value = null;
    reportType.value = 'trends';
    try {
      const result = await insightsApi.runTrends({
        datasetId: selectedTable.value,
        sourceId: selectedSourceId.value,
      });
      handleResult(result);
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Analysis failed';
      tree.value = null;
    } finally {
      loading.value = false;
    }
  }

  function handleResult(result: Awaited<ReturnType<typeof insightsApi.runReview>> | null): void {
    if (!result) {
      return;
    }
    tree.value = result.tree;
    executionTimeMs.value = result.executionTimeMs;
    nodeCount.value = result.nodeCount;
    findingCount.value = result.findingCount;
    firstLevelCount.value = result.firstLevelCount;
    deeperCount.value = result.deeperCount;
    totalCandidates.value = result.totalCandidates;
    showAll.value = false;
  }

  function reset(): void {
    selectedSourceId.value = null;
    selectedTable.value = null;
    resetState();
    loading.value = false;
  }

  return {
    availableMeasures,
    availableTypes,
    cadence,
    deeperCount,
    error,
    executionTimeMs,
    filters,
    findingCount,
    firstLevelCount,
    loading,
    nodeCount,
    reportType,
    reset,
    runReview,
    runTrends,
    selectTable,
    selectedSourceId,
    selectedTable,
    showAll,
    totalCandidates,
    tree,
    visibleRoots,
  };
});

function nodeDirection(n: AnalysisNode): 'up' | 'down' | null {
  const a = n.analysis;
  switch (a.type) {
    case 'Anomaly': {
      return a.z_score > 0 ? 'up' : 'down';
    }
    case 'PeriodComparison':
    case 'PeriodAnomaly':
    case 'Segment': {
      return a.change_percent > 0 ? 'up' : 'down';
    }
    case 'Trend': {
      return a.direction === 'Increasing' ? 'up' : 'down';
    }
    case 'ForecastDeviation': {
      return a.deviation_percent > 0 ? 'up' : 'down';
    }
    case 'OutlierCluster': {
      return a.direction === 'spike' ? 'up' : 'down';
    }
    case 'ChangePoint': {
      return a.after_mean > a.before_mean ? 'up' : 'down';
    }
    case 'RankChange': {
      return a.new_rank < a.previous_rank ? 'up' : 'down';
    }
    case 'TopDominance':
    case 'Concentration':
    case 'Correlation':
    case 'DistributionShift':
    case 'MembershipChange':
    case 'Seasonality': {
      return null;
    }
  }
}

function nodeMeasure(n: AnalysisNode): string | null {
  const a = n.analysis;
  switch (a.type) {
    case 'Anomaly':
    case 'ChangePoint':
    case 'Concentration':
    case 'DistributionShift':
    case 'ForecastDeviation':
    case 'PeriodAnomaly':
    case 'PeriodComparison':
    case 'Seasonality':
    case 'Trend': {
      return a.column;
    }
    case 'Segment': {
      return a.target_column;
    }
    case 'Correlation': {
      return a.column_a;
    }
    case 'OutlierCluster': {
      return a.columns[0] ?? null;
    }
    case 'RankChange':
    case 'TopDominance': {
      return a.measure;
    }
    case 'MembershipChange': {
      return null;
    }
  }
}
