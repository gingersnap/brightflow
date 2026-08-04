/**
 * View-level UI preferences: view mode, chart type, text size, section
 * collapse.
 *
 * Persisted via useLocalStorage with validating serializers — a stored value
 * from an older build that no longer names a valid mode falls back to the
 * default instead of putting the UI into an unrenderable state.
 */

import { useLocalStorage } from '@vueuse/core';
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
  return ['small', 'default', 'large'].includes(s);
}

/** Serializer that validates on read and falls back to the default. */
function validated<T extends string>(guard: (s: string) => s is T, fallback: T) {
  return {
    read: (raw: string): T => (guard(raw) ? raw : fallback),
    write: (value: T): string => value,
  };
}

function readTextSize(raw: string): TextSize {
  // Migrate legacy values from the old 2-size system.
  if (raw === 'compact') {
    return 'small';
  }
  if (raw === 'comfortable') {
    return 'default';
  }
  return isTextSize(raw) ? raw : 'default';
}

function applyTextSize(size: TextSize): void {
  const root = document.documentElement;
  root.classList.toggle('text-small', size === 'small');
  root.classList.toggle('text-large', size === 'large');
}

export const useUiStore = defineStore('ui', () => {
  const viewMode = useLocalStorage<ViewMode>('brightflow-view-mode', 'table', {
    serializer: validated(isViewMode, 'table'),
  });
  const chartType = useLocalStorage<ChartType>('brightflow-chart-type', 'bar', {
    serializer: validated(isChartType, 'bar'),
  });
  const textSize = useLocalStorage<TextSize>('brightflow-text-size', 'default', {
    serializer: { read: readTextSize, write: (v: TextSize) => v },
  });

  // Section collapsed states (Filter collapsed by default, others open)
  const filterCollapsed = ref(true);
  const summarizeCollapsed = ref(false);
  const resultsCollapsed = ref(false);

  // Track if we've shown pivot results yet (for auto-switch)
  const hasShownPivotResults = ref(false);

  watch(textSize, applyTextSize);
  // Apply on init
  applyTextSize(textSize.value);

  const collapsedBySection: Record<SectionName, typeof filterCollapsed> = {
    filter: filterCollapsed,
    results: resultsCollapsed,
    summarize: summarizeCollapsed,
  };

  // Actions
  function toggleSection(section: SectionName): void {
    const collapsed = collapsedBySection[section];
    collapsed.value = !collapsed.value;
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
    onPivotResults,
    resetForNewDataset,
    resultsCollapsed,
    summarizeCollapsed,
    textSize,
    toggleSection,
    viewMode,
  };
});
