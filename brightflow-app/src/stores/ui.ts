import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import type { ViewMode, ChartType } from '@/types';

export type AppMode = 'explore' | 'insights';
type SectionName = 'filter' | 'summarize' | 'results';

export const useUiStore = defineStore('ui', () => {
  // App mode: top-level navigation between Explore and Insights
  const storedMode = localStorage.getItem('brightflow-app-mode');
  const initialConnect = storedMode === 'connect';
  const appMode = ref<AppMode>(
    initialConnect ? 'explore' : ((storedMode as AppMode | null) ?? 'explore'),
  );

  // Connect is separate from Explore/Insights
  const showConnect = ref(initialConnect);

  // View mode: 'table' | 'pivot' | 'chart' | 'split'
  const viewMode = ref<ViewMode>(
    (localStorage.getItem('brightflow-view-mode') as ViewMode | null) ?? 'table',
  );

  // Chart type: 'bar' | 'line' | 'pie' | 'scatter'
  const chartType = ref<ChartType>(
    (localStorage.getItem('brightflow-chart-type') as ChartType | null) ?? 'bar',
  );

  // Section collapsed states (Filter collapsed by default, others open)
  const filterCollapsed = ref(true);
  const summarizeCollapsed = ref(false);
  const resultsCollapsed = ref(false);

  // Track if we've shown pivot results yet (for auto-switch)
  const hasShownPivotResults = ref(false);

  // Persist preferences
  watch(appMode, (val) => localStorage.setItem('brightflow-app-mode', val));
  watch(showConnect, (val) => {
    if (val) {
      localStorage.setItem('brightflow-app-mode', 'connect');
    }
  });
  watch(viewMode, (val) => localStorage.setItem('brightflow-view-mode', val));
  watch(chartType, (val) => localStorage.setItem('brightflow-chart-type', val));

  // Actions
  function setAppMode(mode: AppMode): void {
    appMode.value = mode;
    showConnect.value = false;
  }

  function setShowConnect(val: boolean): void {
    showConnect.value = val;
  }

  function setViewMode(mode: ViewMode): void {
    viewMode.value = mode;
  }

  function setChartType(type: ChartType): void {
    chartType.value = type;
  }

  function toggleSection(section: SectionName): void {
    if (section === 'filter') filterCollapsed.value = !filterCollapsed.value;
    if (section === 'summarize') summarizeCollapsed.value = !summarizeCollapsed.value;
    if (section === 'results') resultsCollapsed.value = !resultsCollapsed.value;
  }

  // Called when pivot results are received
  function onPivotResults(): void {
    if (!hasShownPivotResults.value) {
      hasShownPivotResults.value = true;
      viewMode.value = 'pivot';
    }
  }

  // Reset for new dataset
  function resetForNewDataset(): void {
    hasShownPivotResults.value = false;
    viewMode.value = 'table';
  }

  return {
    appMode,
    showConnect,
    viewMode,
    chartType,
    filterCollapsed,
    summarizeCollapsed,
    resultsCollapsed,
    hasShownPivotResults,
    setAppMode,
    setShowConnect,
    setViewMode,
    setChartType,
    toggleSection,
    onPivotResults,
    resetForNewDataset,
  };
});
