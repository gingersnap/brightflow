/**
 * The loaded dataset: its columns, their stored semantics, and its identity.
 *
 * Client state only — the load itself (fetch + initial rows) lives in the
 * explore tool; this store receives the response and resets UI state keyed to
 * the previous table's columns. This is not the whole reset — `resetAllStores`
 * covers the rest — so treat it as the minimum this store owes its own
 * consumers, not as a guarantee that nothing stale survives anywhere.
 *
 * Semantics arrive with the load response and are patched in place from
 * applied `set_column_*` / `set_kpi` / `set_table_settings` action events
 * (`applySemanticAction`), so a rename, a role change or a new analysis
 * period shows without reloading the table. Every Explore surface that names
 * a column goes through `labelFor`.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { ColumnInfo, LoadTableResponse } from '@/types';
import type { ActionLogEntry, ColumnRole, Polarity, TimeGranularity } from '@/types/generated';
import { isNumericType, isTemporalType } from '@/utils/dtype';
import { humanizeColumn } from '@/utils/format';

import { useUiStore } from './ui';

/** The engine's default period; used when a table has no setting of its own. */
const DEFAULT_TIME_GRANULARITY: TimeGranularity = 'week';

/** The `params` of a logged column-semantic action, as the bus serialises it. */
interface SemanticActionParams {
  kind: string;
  source_id: string;
  table: string;
  column: string;
  role?: ColumnRole;
  label?: string | null;
  description?: string | null;
  is_kpi?: boolean;
  polarity?: Polarity;
}

const SEMANTIC_KINDS = new Set([
  'set_column_role',
  'set_column_label',
  'set_column_description',
  'set_kpi',
  'set_column_polarity',
]);

/** The `params` of a logged `set_table_settings` action. */
interface TableSettingsParams {
  kind: 'set_table_settings';
  source_id: string;
  table: string;
  time_granularity?: TimeGranularity;
}

function isTableSettingsParams(params: unknown): params is TableSettingsParams {
  return (
    isRecord(params) &&
    params['kind'] === 'set_table_settings' &&
    typeof params['table'] === 'string'
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isSemanticParams(params: unknown): params is SemanticActionParams {
  if (!isRecord(params)) {
    return false;
  }
  const kind = params['kind'];
  return (
    typeof kind === 'string' &&
    SEMANTIC_KINDS.has(kind) &&
    typeof params['table'] === 'string' &&
    typeof params['column'] === 'string'
  );
}

/** A column is a time axis by stored role, or by logical type when it has none. */
export function isTimeColumn(col: Pick<ColumnInfo, 'role' | 'datatype'>): boolean {
  return col.role === 'time' || (col.role == null && isTemporalType(col.datatype));
}

export const useDatasetStore = defineStore('dataset', () => {
  // State - current dataset
  const id = ref('default');
  const name = ref<string | null>(null);
  const rowCount = ref<number | null>(null);
  const columns = ref<ColumnInfo[]>([]);
  const timeGranularity = ref<TimeGranularity>(DEFAULT_TIME_GRANULARITY);
  const loading = ref(false);
  const error = ref<string | null>(null);

  // Computed
  const numericColumns = computed(() => columns.value.filter((c) => isNumericType(c.datatype)));

  /** Columns Explore offers: everything not marked `ignored`. */
  const visibleColumns = computed(() => columns.value.filter((c) => c.role !== 'ignored'));

  const timeColumns = computed(() => columns.value.filter((c) => isTimeColumn(c)));

  const hasData = computed(() => columns.value.length > 0);

  function columnByName(columnName: string): ColumnInfo | undefined {
    return columns.value.find((c) => c.name === columnName);
  }

  /** Stored label, else the humanised column name; unknown names humanise too. */
  function labelFor(columnName: string): string {
    const col = columnByName(columnName);
    if (col?.label != null && col.label !== '') {
      return col.label;
    }
    return humanizeColumn(columnName);
  }

  // Set state from a LoadTableResponse (REST-based, no WS needed)
  function setFromLoadResponse(response: LoadTableResponse): void {
    id.value = response.id;
    name.value = response.name;
    rowCount.value = response.rowCount;
    columns.value = response.columns;
    timeGranularity.value = response.timeGranularity ?? DEFAULT_TIME_GRANULARITY;
    loading.value = false;
    error.value = null;

    // Reset UI state for new dataset
    const uiStore = useUiStore();
    uiStore.resetForNewDataset();
  }

  /**
   * Patch one column — or the table's analysis period — from an applied
   * semantic action on the loaded table. Returns whether anything changed.
   * Entries for other tables, other kinds, or non-applied statuses are
   * ignored; an undone entry is ignored too — the undo lands as its own
   * applied write on the row, and the reload path remains the source of
   * truth if a frame is missed.
   */
  function applySemanticAction(entry: ActionLogEntry): boolean {
    if (entry.status !== 'applied') {
      return false;
    }
    if (isTableSettingsParams(entry.params)) {
      const settings = entry.params;
      if (name.value == null || settings.table !== name.value) {
        return false;
      }
      if (settings.time_granularity == null) {
        return false;
      }
      timeGranularity.value = settings.time_granularity;
      return true;
    }
    if (!isSemanticParams(entry.params)) {
      return false;
    }
    const params = entry.params;
    if (name.value == null || params.table !== name.value) {
      return false;
    }
    const col = columnByName(params.column);
    if (col == null) {
      return false;
    }
    switch (params.kind) {
      case 'set_column_role': {
        if (params.role == null) {
          return false;
        }
        col.role = params.role;
        if (params.role !== 'measure') {
          col.isKpi = false;
        }
        return true;
      }
      case 'set_column_label': {
        const label = params.label?.trim() ?? '';
        col.label = label === '' ? null : label;
        return true;
      }
      case 'set_column_description': {
        const description = params.description?.trim() ?? '';
        if (description === '') {
          delete col.description;
        } else {
          col.description = description;
        }
        return true;
      }
      case 'set_kpi': {
        col.isKpi = params.is_kpi ?? false;
        return true;
      }
      case 'set_column_polarity': {
        if (params.polarity == null) {
          return false;
        }
        col.polarity = params.polarity;
        return true;
      }
      default: {
        return false;
      }
    }
  }

  function reset(): void {
    id.value = 'default';
    name.value = null;
    rowCount.value = null;
    columns.value = [];
    timeGranularity.value = DEFAULT_TIME_GRANULARITY;
    error.value = null;
  }

  return {
    // Current dataset state
    id,
    name,
    rowCount,
    columns,
    timeGranularity,
    loading,
    error,
    // Computed
    numericColumns,
    visibleColumns,
    timeColumns,
    hasData,
    // Lookups
    columnByName,
    labelFor,
    // Actions
    setFromLoadResponse,
    applySemanticAction,
    reset,
  };
});
