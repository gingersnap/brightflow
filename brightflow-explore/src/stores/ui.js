import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export const useUiStore = defineStore('ui', () => {
  // View mode: 'table' | 'pivot' | 'chart' | 'split'
  const viewMode = ref(localStorage.getItem('lighthouse-view-mode') || 'table')

  // Chart type: 'bar' | 'line' | 'pie' | 'scatter'
  const chartType = ref(localStorage.getItem('lighthouse-chart-type') || 'bar')

  // Section collapsed states (Filter collapsed by default, others open)
  const filterCollapsed = ref(true)
  const summarizeCollapsed = ref(false)
  const resultsCollapsed = ref(false)

  // Track if we've shown pivot results yet (for auto-switch)
  const hasShownPivotResults = ref(false)

  // Persist preferences
  watch(viewMode, (val) => localStorage.setItem('lighthouse-view-mode', val))
  watch(chartType, (val) => localStorage.setItem('lighthouse-chart-type', val))

  // Actions
  function setViewMode(mode) {
    viewMode.value = mode
  }

  function setChartType(type) {
    chartType.value = type
  }

  function toggleSection(section) {
    if (section === 'filter') filterCollapsed.value = !filterCollapsed.value
    if (section === 'summarize') summarizeCollapsed.value = !summarizeCollapsed.value
    if (section === 'results') resultsCollapsed.value = !resultsCollapsed.value
  }

  // Called when pivot results are received
  function onPivotResults() {
    if (!hasShownPivotResults.value) {
      hasShownPivotResults.value = true
      viewMode.value = 'pivot'
    }
  }

  // Reset for new dataset
  function resetForNewDataset() {
    hasShownPivotResults.value = false
    viewMode.value = 'table'
  }

  return {
    viewMode,
    chartType,
    filterCollapsed,
    summarizeCollapsed,
    resultsCollapsed,
    hasShownPivotResults,
    setViewMode,
    setChartType,
    toggleSection,
    onPivotResults,
    resetForNewDataset
  }
})
