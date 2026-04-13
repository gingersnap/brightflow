import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import { type ToolId, type UnifiedSource, toolsForSource } from '@/types';

function isToolId(s: string): s is ToolId {
  return ['dashboard', 'funnels', 'retention', 'users', 'explore', 'insights'].includes(s);
}

export const useSourceStore = defineStore('source', () => {
  // Persisted state
  const storedSourceId = localStorage.getItem('brightflow-source-id');
  const selectedSourceId = ref<string | null>(storedSourceId);

  const storedTool = localStorage.getItem('brightflow-selected-tool');
  const selectedTool = ref<ToolId>(
    storedTool != null && isToolId(storedTool) ? storedTool : 'dashboard',
  );

  const selectedTableName = ref<string | null>(null);

  const storedPeriod = localStorage.getItem('brightflow-period');
  const period = ref(storedPeriod ?? '30d');

  // The actual source object — set externally by component layer via setSourceData
  const sourcesData = ref<UnifiedSource[]>([]);

  // Computed
  const selectedSource = computed(
    () => sourcesData.value.find((s) => s.id === selectedSourceId.value) ?? null,
  );

  const availableTools = computed(() => {
    const source = selectedSource.value;
    if (!source) {
      return [];
    }
    return toolsForSource(source);
  });

  // Persist
  watch(selectedSourceId, (val) => {
    if (val != null) {
      localStorage.setItem('brightflow-source-id', val);
    } else {
      localStorage.removeItem('brightflow-source-id');
    }
  });
  watch(selectedTool, (val) => {
    localStorage.setItem('brightflow-selected-tool', val);
  });
  watch(period, (val) => {
    localStorage.setItem('brightflow-period', val);
  });

  // Actions
  function setSourcesData(sources: UnifiedSource[]): void {
    sourcesData.value = sources;
  }

  function selectSource(id: string): void {
    selectedSourceId.value = id;
    selectedTool.value = 'dashboard';
    selectedTableName.value = null;
  }

  function selectTool(tool: ToolId): void {
    selectedTool.value = tool;
    if (tool !== 'explore') {
      selectedTableName.value = null;
    }
  }

  function selectTable(name: string | null): void {
    selectedTableName.value = name;
  }

  function clearSource(): void {
    selectedSourceId.value = null;
    selectedTableName.value = null;
  }

  function reset(): void {
    selectedSourceId.value = null;
    selectedTool.value = 'dashboard';
    selectedTableName.value = null;
    period.value = '30d';
    sourcesData.value = [];
    localStorage.removeItem('brightflow-source-id');
    localStorage.removeItem('brightflow-selected-tool');
    localStorage.removeItem('brightflow-period');
  }

  return {
    availableTools,
    clearSource,
    period,
    reset,
    selectSource,
    selectTable,
    selectTool,
    selectedSource,
    selectedSourceId,
    selectedTableName,
    selectedTool,
    setSourcesData,
    sourcesData,
  };
});
