import { defineStore } from 'pinia';
import { ref, watch } from 'vue';

import type { ChartType, ViewMode } from '@/types';

type SectionName = 'filter' | 'summarize' | 'results';

function isViewMode(s: string): s is ViewMode {
  return ['table', 'pivot', 'chart', 'split', 'number'].includes(s);
}

function isChartType(s: string): s is ChartType {
  return ['bar', 'line', 'pie', 'scatter'].includes(s);
}

export const useUiStore = defineStore('ui', () => {
  // Connect view toggle
  const storedMode = localStorage.getItem('brightflow-app-mode');
  const showConnect = ref(storedMode === 'connect');

  // System observability view
  const showSystem = ref(storedMode === 'system');

  // View mode: 'table' | 'pivot' | 'chart' | 'split'
  const storedViewMode = localStorage.getItem('brightflow-view-mode');
  const viewMode = ref<ViewMode>(
    storedViewMode != null && isViewMode(storedViewMode) ? storedViewMode : 'table',
  );

  // Chart type: 'bar' | 'line' | 'pie' | 'scatter'
  const storedChartType = localStorage.getItem('brightflow-chart-type');
  const chartType = ref<ChartType>(
    storedChartType != null && isChartType(storedChartType) ? storedChartType : 'bar',
  );

  // Section collapsed states (Filter collapsed by default, others open)
  const filterCollapsed = ref(true);
  const summarizeCollapsed = ref(false);
  const resultsCollapsed = ref(false);

  // Sidebar collapsed state
  const storedSidebarCollapsed = localStorage.getItem('brightflow-sidebar-collapsed');
  const sidebarCollapsed = ref(storedSidebarCollapsed === 'true');

  // Track if we've shown pivot results yet (for auto-switch)
  const hasShownPivotResults = ref(false);

  // Persist preferences
  watch(sidebarCollapsed, (val) => {
    localStorage.setItem('brightflow-sidebar-collapsed', String(val));
  });
  watch(showConnect, (val) => {
    if (val) {
      localStorage.setItem('brightflow-app-mode', 'connect');
    }
  });
  watch(showSystem, (val) => {
    if (val) {
      localStorage.setItem('brightflow-app-mode', 'system');
    }
  });
  watch(viewMode, (val) => {
    localStorage.setItem('brightflow-view-mode', val);
  });
  watch(chartType, (val) => {
    localStorage.setItem('brightflow-chart-type', val);
  });

  // Actions
  function setShowConnect(val: boolean): void {
    showConnect.value = val;
    if (val) {
      showSystem.value = false;
    }
  }

  function setShowSystem(val: boolean): void {
    showSystem.value = val;
    if (val) {
      showConnect.value = false;
    }
  }

  function setViewMode(mode: ViewMode): void {
    viewMode.value = mode;
  }

  function setChartType(type: ChartType): void {
    chartType.value = type;
  }

  function toggleSection(section: SectionName): void {
    if (section === 'filter') {
      filterCollapsed.value = !filterCollapsed.value;
    }
    if (section === 'summarize') {
      summarizeCollapsed.value = !summarizeCollapsed.value;
    }
    if (section === 'results') {
      resultsCollapsed.value = !resultsCollapsed.value;
    }
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
    chartType,
    filterCollapsed,
    hasShownPivotResults,
    onPivotResults,
    resetForNewDataset,
    resultsCollapsed,
    setChartType,
    sidebarCollapsed,
    setShowConnect,
    setShowSystem,
    setViewMode,
    showConnect,
    showSystem,
    summarizeCollapsed,
    toggleSection,
    viewMode,
  };
});
