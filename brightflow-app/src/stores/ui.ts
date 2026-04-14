import { defineStore } from 'pinia';
import { ref, watch } from 'vue';

import type { ChartType, TextSize, ViewMode } from '@/types';

type SectionName = 'filter' | 'summarize' | 'results';

function isViewMode(s: string): s is ViewMode {
  return ['table', 'pivot', 'chart', 'split', 'number'].includes(s);
}

function isChartType(s: string): s is ChartType {
  return ['bar', 'line', 'pie', 'scatter'].includes(s);
}

function isTextSize(s: string): s is TextSize {
  return ['compact', 'comfortable'].includes(s);
}

export const useUiStore = defineStore('ui', () => {
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

  // Text size preference
  const storedTextSize = localStorage.getItem('brightflow-text-size');
  const textSize = ref<TextSize>(
    storedTextSize != null && isTextSize(storedTextSize) ? storedTextSize : 'compact',
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
  watch(textSize, (val) => {
    localStorage.setItem('brightflow-text-size', val);
    document.documentElement.classList.toggle('text-comfortable', val === 'comfortable');
  });
  // Apply on init
  document.documentElement.classList.toggle('text-comfortable', textSize.value === 'comfortable');

  watch(viewMode, (val) => {
    localStorage.setItem('brightflow-view-mode', val);
  });
  watch(chartType, (val) => {
    localStorage.setItem('brightflow-chart-type', val);
  });

  // Actions
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

  function toggleTextSize(): void {
    textSize.value = textSize.value === 'compact' ? 'comfortable' : 'compact';
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
    setViewMode,
    summarizeCollapsed,
    textSize,
    toggleSection,
    toggleTextSize,
    viewMode,
  };
});
