<script setup lang="ts">
/**
 * The "Summarize" section: drag columns from the sidebar into row / column /
 * value buckets to build a pivot. A watcher enforces Polars' pivot shape
 * (columns require rows; a lone value field auto-adds a row) so the store
 * never holds an un-runnable config, and changes auto-execute through a
 * 300ms debounce — there is no run button.
 *
 * The column list is the semantic layer's face in Explore: it shows the
 * dataset store's visible columns (ignored ones are hidden), labelled and
 * described from their stored semantics, with an icon by role and a KPI
 * badge. A dropped column carries its role into the bucket. Right-clicking
 * a column opens the semantics menu (`columnMenu.ts`); "Show ignored"
 * reveals hidden columns greyed so one can be un-ignored.
 */

import type { ContextMenuItem } from '@nuxt/ui';
import { watchDebounced } from '@vueuse/core';
import { computed, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import draggable from 'vuedraggable';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { useColumnSemantics } from '@/composables/useColumnSemantics';
import { useWsQuery } from '@/composables/useWsQuery';
import { useDatasetStore } from '@/stores/dataset';
import { type DroppedColumn, usePivotStore } from '@/stores/pivot';
import { useQueryStore } from '@/stores/query';
import { useUiStore } from '@/stores/ui';
import type { PivotField } from '@/types';
import type { ColumnInfo, ColumnRole } from '@/types/generated';
import { isNumericDtype, isStringDtype } from '@/utils/dtype';

import BucketDropzone from '../pivot/BucketDropzone.vue';
import { columnMenuItems } from './columnMenu';

interface ColumnItem {
  name: string;
  dtype: string;
  id: string;
  role: ColumnRole | null;
  label: string;
  description: string | undefined;
  isKpi: boolean;
  isIgnored: boolean;
  isNumeric: boolean;
  isString: boolean;
}

const pivotStore = usePivotStore();
const queryStore = useQueryStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();
const { executePivot, canExecute } = useWsQuery();

const summarizeOpen = computed({
  get: () => !uiStore.summarizeCollapsed,
  set: () => uiStore.toggleSection('summarize'),
});

const showIgnored = ref(false);
const hasIgnored = computed(() => datasetStore.columns.some((c) => c.role === 'ignored'));

// Columns for the sidebar: the visible ones, labelled from their semantics
const columns = computed((): ColumnItem[] =>
  (showIgnored.value ? datasetStore.columns : datasetStore.visibleColumns).map((col) => ({
    description: col.description,
    dtype: col.dtype,
    id: col.name,
    isIgnored: col.role === 'ignored',
    isKpi: col.isKpi === true,
    isNumeric: isNumericDtype(col.dtype),
    isString: isStringDtype(col.dtype),
    label: datasetStore.labelFor(col.name),
    name: col.name,
    role: col.role,
  })),
);

// ── Column context menu ──────────────────────────────────────────────────

const route = useRoute();
const semantics = useColumnSemantics();
const menuOpen = ref(false);
const menuColumn = ref<ColumnInfo | null>(null);

function openColumnMenu(e: MouseEvent, item: ColumnItem): void {
  e.preventDefault();
  menuColumn.value = datasetStore.columnByName(item.name) ?? null;
  menuOpen.value = menuColumn.value != null;
}

const menuItems = computed<ContextMenuItem[][]>(() => {
  const column = menuColumn.value;
  const sourceId = String(route.params['sourceId'] ?? '');
  const table = datasetStore.name;
  if (column == null || sourceId === '' || table == null) {
    return [];
  }
  const scope = { sourceId, table };
  return columnMenuItems(column, {
    clearDescription: () => void semantics.clearDescription(scope, column),
    clearLabel: () => void semantics.clearLabel(scope, column),
    describe: () => void semantics.describe(scope, column),
    rename: () => void semantics.rename(scope, column),
    setKpi: (isKpi) => void semantics.setKpi(scope, column, isKpi),
    setPolarity: (polarity) => void semantics.setPolarity(scope, column, polarity),
    setRole: (role) => void semantics.setRole(scope, column, role),
  });
});

const ROLE_ICONS: Record<ColumnRole, string> = {
  dimension: 'i-lucide-tag',
  entity: 'i-lucide-user',
  ignored: 'i-lucide-eye-off',
  measure: 'i-lucide-hash',
  time: 'i-lucide-calendar',
};

// Icon by role when the column has one, by dtype otherwise
function getTypeIcon(col: ColumnItem): string {
  if (col.role != null) {
    return ROLE_ICONS[col.role];
  }
  if (col.isNumeric) {
    return 'i-lucide-hash';
  }
  if (col.isString) {
    return 'i-lucide-type';
  }
  return 'i-lucide-circle-help';
}

// Clone function for draggable
function cloneColumn(col: ColumnItem): ColumnItem & { column: string } {
  return {
    ...col,
    column: col.name,
  };
}

// Enforce Polars pivot rules
watch(
  () => ({
    columns: pivotStore.columnFields.length,
    rows: pivotStore.rowFields.length,
    values: pivotStore.valueFields.length,
  }),
  ({ values, rows, columns: colCount }) => {
    // Rule: If columns exist but rows don't, move columns to rows
    if (colCount > 0 && rows === 0) {
      const colField = pivotStore.columnFields[0];
      if (colField) {
        pivotStore.removeColumnField(colField.id);
        pivotStore.addRowField(colField);
      }
      return;
    }

    // Rule: If only values exist, auto-add a row
    if (values > 0 && rows === 0 && colCount === 0) {
      const valueField = pivotStore.valueFields[0];
      if (valueField) {
        const aggFunc = valueField.aggregation ?? 'count';

        if (aggFunc === 'count') {
          pivotStore.addRowField(valueField);
        } else {
          const stringCol = datasetStore.visibleColumns.find(
            (c) => isStringDtype(c.dtype) && c.name !== valueField.column,
          );
          if (stringCol) {
            pivotStore.addRowField({
              column: stringCol.name,
              dtype: stringCol.dtype,
              role: stringCol.role,
            });
          } else {
            pivotStore.addRowField(valueField);
          }
        }
      }
    }
  },
  { deep: true },
);

// Auto-execute when configuration changes (debounced; cleans up on unmount)
watchDebounced(
  () => [
    pivotStore.rowFields.map((f) => `${f.column}:${f.granularity}`),
    pivotStore.columnFields.map((f) => `${f.column}:${f.granularity}`),
    pivotStore.valueFields.map((f) => `${f.column}:${f.aggregation}`),
    queryStore.filters.map((f) => `${f.column}:${f.op}:${f.value}`),
    queryStore.sections.filter.enabled,
    queryStore.sortBy,
    queryStore.sortDescending,
    queryStore.sections.sort.enabled,
  ],
  () => {
    if (canExecute()) {
      executePivot();
    }
  },
  { debounce: 300, deep: true },
);

// Bucket handlers
function handleAddRow(field: DroppedColumn): void {
  pivotStore.addRowField(field);
}

function handleAddColumn(field: DroppedColumn): void {
  pivotStore.addColumnField(field);
}

function handleAddValue(field: DroppedColumn): void {
  pivotStore.addValueField(field);
}

function handleReorderRows(newOrder: PivotField[]): void {
  pivotStore.reorderRowFields(newOrder);
}

/** Row/column chips update their sort or, for time fields, their period. */
function handleFieldUpdate(id: string, updates: Partial<PivotField>): void {
  if (updates.sort != null) {
    pivotStore.setFieldSort(id, updates.sort);
  }
  if (updates.granularity != null) {
    pivotStore.setFieldGranularity(id, updates.granularity);
  }
}

function handleReorderValues(newOrder: PivotField[]): void {
  pivotStore.reorderValueFields(newOrder);
}

// Sort handlers
function toggleSort(): void {
  queryStore.toggleSection('sort');
}

function handleSortColumnChange(column: string): void {
  queryStore.sortBy = column;
}

function toggleSortDirection() {
  queryStore.sortDescending = !queryStore.sortDescending;
}

// Column options for sort
const sortColumnOptions = computed(() =>
  datasetStore.visibleColumns.map((col) => ({
    label: datasetStore.labelFor(col.name),
    value: col.name,
  })),
);
</script>

<template>
  <CollapsibleSection v-model:open="summarizeOpen" class="border-b border-default">
    <template #title>
      <h2 class="text-sm font-medium text-default">Summarize</h2>
      <span v-if="pivotStore.isConfigured" class="text-xs text-muted">
        ({{ pivotStore.valueFields.length }} value{{
          pivotStore.valueFields.length !== 1 ? 's' : ''
        }})
      </span>
    </template>

    <template #actions="{ open }">
      <template v-if="open">
        <!-- Settings toggles -->
        <label class="flex cursor-pointer items-center gap-1.5 text-sm text-muted">
          <USwitch v-model="pivotStore.showSubtotals" size="xs" />
          Subtotals
        </label>
        <label class="flex cursor-pointer items-center gap-1.5 text-sm text-muted">
          <USwitch v-model="pivotStore.showColumnTotals" size="xs" />
          Totals
        </label>
        <label class="flex cursor-pointer items-center gap-1.5 text-sm text-muted">
          <USwitch v-model="pivotStore.showConditionalFormatting" size="xs" />
          Heatmap
        </label>

        <!-- Decimals -->
        <div class="flex items-center gap-1.5 text-sm text-muted">
          <span>Dec:</span>
          <USelectMenu
            v-model="pivotStore.decimalPlaces"
            :items="[
              { label: '0', value: 0 },
              { label: '1', value: 1 },
              { label: '2', value: 2 },
              { label: '3', value: 3 },
            ]"
            value-key="value"
            size="xs"
            class="w-12"
          />
        </div>

        <!-- Reset -->
        <UButton variant="ghost" size="md" @click="pivotStore.reset()">
          <UIcon name="i-lucide-rotate-ccw" class="h-3 w-3" />
        </UButton>
      </template>
    </template>

    <!-- Content -->
    <div class="flex border-t border-default bg-muted/10">
      <!-- Column List (left side) -->
      <div class="w-48 border-r border-default bg-muted/20 p-3">
        <UContextMenu v-model:open="menuOpen" :items="menuItems">
          <div v-if="columns.length" class="max-h-48 space-y-1 overflow-y-auto">
            <draggable
              :list="columns"
              :group="{ name: 'columns', pull: 'clone', put: false }"
              :clone="cloneColumn"
              :sort="false"
              item-key="id"
              class="space-y-1"
            >
              <template #item="{ element }">
                <div
                  class="group flex cursor-grab items-center gap-2 rounded-md bg-default/50 px-2 py-1.5 text-sm transition-colors hover:bg-default active:cursor-grabbing"
                  :class="{ 'opacity-50': element.isIgnored }"
                  @contextmenu="openColumnMenu($event, element)"
                >
                  <UIcon
                    name="i-lucide-grip-vertical"
                    class="h-3 w-3 text-muted/30 group-hover:text-muted/60"
                  />
                  <UIcon :name="getTypeIcon(element)" class="h-3 w-3 text-muted" />
                  <UTooltip :text="element.description" :disabled="!element.description">
                    <span class="flex-1 truncate">{{ element.label }}</span>
                  </UTooltip>
                  <UBadge v-if="element.isKpi" size="md" variant="subtle" color="primary">
                    KPI
                  </UBadge>
                </div>
              </template>
            </draggable>
          </div>

          <div v-else class="py-4 text-sm text-muted/60">No columns loaded</div>
        </UContextMenu>

        <label
          v-if="hasIgnored"
          class="mt-2 flex cursor-pointer items-center gap-1.5 text-sm text-muted"
        >
          <USwitch v-model="showIgnored" size="xs" />
          Show ignored
        </label>
      </div>

      <!-- Buckets (right side) -->
      <div class="flex-1 p-3">
        <!-- Three Bucket Layout -->
        <div class="mb-3 grid grid-cols-3 gap-3">
          <BucketDropzone
            title="Rows"
            bucket="rows"
            :fields="pivotStore.rowFields"
            :show-sort="true"
            @add="handleAddRow"
            @remove="pivotStore.removeRowField"
            @reorder="handleReorderRows"
            @update="handleFieldUpdate"
          />

          <div class="relative">
            <!-- Flip button between Rows and Columns -->
            <button
              v-if="pivotStore.rowFields.length > 0 || pivotStore.columnFields.length > 0"
              class="absolute top-8 -left-5 z-10 rounded-full bg-muted/50 p-1 text-muted transition-colors hover:bg-muted hover:text-default"
              title="Flip rows and columns"
              @click="pivotStore.flipRowsAndColumns()"
            >
              <UIcon name="i-lucide-arrow-left-right" class="h-3 w-3" />
            </button>
            <BucketDropzone
              title="Columns"
              bucket="columns"
              :fields="pivotStore.columnFields"
              :max-items="1"
              :show-sort="true"
              :disabled="pivotStore.rowFields.length === 0"
              disabled-message="Add rows first"
              @add="handleAddColumn"
              @remove="pivotStore.removeColumnField"
              @update="handleFieldUpdate"
            />
          </div>

          <BucketDropzone
            title="Values"
            bucket="values"
            :fields="pivotStore.valueFields"
            :show-aggregation="true"
            @add="handleAddValue"
            @remove="pivotStore.removeValueField"
            @reorder="handleReorderValues"
            @update="pivotStore.updateValueField"
          />
        </div>

        <!-- Sort Row -->
        <div class="flex items-center gap-3 border-t border-default/50 pt-2">
          <button
            class="flex items-center gap-1.5 text-sm"
            :class="queryStore.sections.sort.enabled ? 'text-muted' : 'text-muted/50'"
            @click="toggleSort"
          >
            <UIcon name="i-lucide-arrow-up-down" class="h-3.5 w-3.5" />
            Sort
          </button>

          <template v-if="queryStore.sections.sort.enabled">
            <USelectMenu
              :model-value="queryStore.sortBy ?? ''"
              :items="sortColumnOptions"
              placeholder="Sort by..."
              value-key="value"
              size="xs"
              class="w-32"
              @update:model-value="(val: string) => handleSortColumnChange(val)"
            />

            <UButton
              v-if="queryStore.sortBy"
              variant="ghost"
              size="md"
              @click="toggleSortDirection"
            >
              <component
                :is="queryStore.sortDescending ? 'i-lucide-arrow-down' : 'i-lucide-arrow-up'"
                class="mr-1 h-3.5 w-3.5"
              />
              {{ queryStore.sortDescending ? 'DESC' : 'ASC' }}
            </UButton>
          </template>

          <!-- Help text -->
          <div v-if="!pivotStore.isConfigured" class="flex-1 text-right text-sm text-muted/60">
            Drag columns into buckets to build your query
          </div>
        </div>
      </div>
    </div>
  </CollapsibleSection>
</template>
