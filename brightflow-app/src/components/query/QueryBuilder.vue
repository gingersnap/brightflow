<script setup lang="ts">
/**
 * The "Summarize" section: drag columns from the sidebar into row / column /
 * value buckets to build a pivot. A watcher enforces Polars' pivot shape
 * (columns require rows; a lone value field auto-adds a row) so the store
 * never holds an un-runnable config, and changes auto-execute through a
 * 300ms debounce — there is no run button.
 */

import {
  ArrowDown,
  ArrowLeftRight,
  ArrowUp,
  ArrowUpDown,
  GripVertical,
  Hash,
  HelpCircle,
  RotateCcw,
  Type,
} from '@lucide/vue';
import { watchDebounced } from '@vueuse/core';
import { type Component, computed, watch } from 'vue';
import draggable from 'vuedraggable';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { useWsQuery } from '@/composables/useWsQuery';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { usePivotStore } from '@/stores/pivot';
import { useQueryStore } from '@/stores/query';
import { useUiStore } from '@/stores/ui';
import type { PivotField } from '@/types';

import BucketDropzone from '../pivot/BucketDropzone.vue';

interface ColumnItem {
  name: string;
  dtype: string;
  id: string;
  isNumeric: boolean;
  isString: boolean;
}

const pivotStore = usePivotStore();
const queryStore = useQueryStore();
const datasetStore = useDatasetStore();
const connectionStore = useConnectionStore();
const uiStore = useUiStore();
const { executePivot, canExecute } = useWsQuery();

const summarizeOpen = computed({
  get: () => !uiStore.summarizeCollapsed,
  set: () => uiStore.toggleSection('summarize'),
});

// Columns for the sidebar
const columns = computed((): ColumnItem[] =>
  datasetStore.columns.map((col) => ({
    ...col,
    id: col.name,
    isNumeric: ['int', 'float', 'decimal', 'number', 'i64', 'f64'].includes(col.dtype),
    isString: ['string', 'text', 'varchar'].includes(col.dtype),
  })),
);

// Get icon for column type
function getTypeIcon(col: ColumnItem): Component {
  if (col.isNumeric) {
    return Hash;
  }
  if (col.isString) {
    return Type;
  }
  return HelpCircle;
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
        pivotStore.addRowField(colField.column, colField.dtype);
      }
      return;
    }

    // Rule: If only values exist, auto-add a row
    if (values > 0 && rows === 0 && colCount === 0) {
      const valueField = pivotStore.valueFields[0];
      if (valueField) {
        const aggFunc = valueField.aggregation ?? 'count';

        if (aggFunc === 'count') {
          pivotStore.addRowField(valueField.column, valueField.dtype);
        } else {
          const stringCol = datasetStore.columns.find(
            (c) => ['string', 'text', 'varchar'].includes(c.dtype) && c.name !== valueField.column,
          );
          if (stringCol) {
            pivotStore.addRowField(stringCol.name, stringCol.dtype);
          } else {
            pivotStore.addRowField(valueField.column, valueField.dtype);
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
    pivotStore.rowFields.map((f) => f.column),
    pivotStore.columnFields.map((f) => f.column),
    pivotStore.valueFields.map((f) => `${f.column}:${f.aggregation}`),
    queryStore.filters.map((f) => `${f.column}:${f.op}:${f.value}`),
    queryStore.sections.filter.enabled,
    queryStore.sortBy,
    queryStore.sortDescending,
    queryStore.sections.sort.enabled,
  ],
  () => {
    if (canExecute() && connectionStore.isConnected && datasetStore.hasData) {
      executePivot();
    }
  },
  { debounce: 300, deep: true },
);

interface FieldParam {
  column: string;
  dtype: string;
}

// Bucket handlers
function handleAddRow(field: FieldParam): void {
  pivotStore.addRowField(field.column, field.dtype);
}

function handleAddColumn(field: FieldParam): void {
  pivotStore.addColumnField(field.column, field.dtype);
}

function handleAddValue(field: FieldParam): void {
  pivotStore.addValueField(field.column, field.dtype);
}

function handleReorderRows(newOrder: PivotField[]): void {
  pivotStore.reorderRowFields(newOrder);
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
  datasetStore.columns.map((col) => ({
    label: col.name,
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
          <RotateCcw class="h-3 w-3" />
        </UButton>
      </template>
    </template>

    <!-- Content -->
    <div class="flex border-t border-default bg-muted/10">
      <!-- Column List (left side) -->
      <div class="w-48 border-r border-default bg-muted/20 p-3">
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
              >
                <GripVertical class="h-3 w-3 text-muted/30 group-hover:text-muted/60" />
                <component :is="getTypeIcon(element)" class="h-3 w-3 text-muted" />
                <span class="flex-1 truncate">{{ element.name }}</span>
              </div>
            </template>
          </draggable>
        </div>

        <div v-else class="py-4 text-sm text-muted/60">No columns loaded</div>
      </div>

      <!-- Buckets (right side) -->
      <div class="flex-1 p-3">
        <!-- Three Bucket Layout -->
        <div class="mb-3 grid grid-cols-3 gap-3">
          <BucketDropzone
            title="Rows"
            bucket="rows"
            :fields="pivotStore.rowFields"
            @add="handleAddRow"
            @remove="pivotStore.removeRowField"
            @reorder="handleReorderRows"
          />

          <div class="relative">
            <!-- Flip button between Rows and Columns -->
            <button
              v-if="pivotStore.rowFields.length > 0 || pivotStore.columnFields.length > 0"
              class="absolute top-8 -left-5 z-10 rounded-full bg-muted/50 p-1 text-muted transition-colors hover:bg-muted hover:text-default"
              title="Flip rows and columns"
              @click="pivotStore.flipRowsAndColumns()"
            >
              <ArrowLeftRight class="h-3 w-3" />
            </button>
            <BucketDropzone
              title="Columns"
              bucket="columns"
              :fields="pivotStore.columnFields"
              :max-items="1"
              :disabled="pivotStore.rowFields.length === 0"
              disabled-message="Add rows first"
              @add="handleAddColumn"
              @remove="pivotStore.removeColumnField"
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
            <ArrowUpDown class="h-3.5 w-3.5" />
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
                :is="queryStore.sortDescending ? ArrowDown : ArrowUp"
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
