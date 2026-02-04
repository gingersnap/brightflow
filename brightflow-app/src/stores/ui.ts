import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import type { ViewMode, ChartType } from '@/types'

type SectionName = 'filter' | 'summarize' | 'results'

export const useUiStore = defineStore('ui', () => {
  // View mode: 'table' | 'pivot' | 'chart' | 'split'
  const viewMode = ref<ViewMode>((localStorage.getItem('brightflow-view-mode') as ViewMode | null) ?? 'table')

  // Chart type: 'bar' | 'line' | 'pie' | 'scatter'
  const chartType = ref<ChartType>((localStorage.getItem('brightflow-chart-type') as ChartType | null) ?? 'bar')

  // Section collapsed states (Filter collapsed by default, others open)
  const filterCollapsed = ref(true)
  const summarizeCollapsed = ref(false)
  const resultsCollapsed = ref(false)

  // Track if we've shown pivot results yet (for auto-switch)
  const hasShownPivotResults = ref(false)

  // Persist preferences
  watch(viewMode, (val) => localStorage.setItem('brightflow-view-mode', val))
  watch(chartType, (val) => localStorage.setItem('brightflow-chart-type', val))

  // Actions
  function setViewMode(mode: ViewMode): void {
    viewMode.value = mode
  }

  function setChartType(type: ChartType): void {
    chartType.value = type
  }

  function toggleSection(section: SectionName): void {
    if (section === 'filter') filterCollapsed.value = !filterCollapsed.value
    if (section === 'summarize') summarizeCollapsed.value = !summarizeCollapsed.value
    if (section === 'results') resultsCollapsed.value = !resultsCollapsed.value
  }

  // Called when pivot results are received
  function onPivotResults(): void {
    if (!hasShownPivotResults.value) {
      hasShownPivotResults.value = true
      viewMode.value = 'pivot'
    }
  }

  // Reset for new dataset
  function resetForNewDataset(): void {
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
