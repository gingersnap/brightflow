<script setup lang="ts">
import {
  ArrowDown,
  ArrowLeftRight,
  ArrowUp,
  ArrowUpDown,
  ChevronDown,
  ChevronRight,
  GripVertical,
  Hash,
  HelpCircle,
  RotateCcw,
  Type,
} from 'lucide-vue-next';
import { type Component, computed, watch } from 'vue';
import draggable from 'vuedraggable';

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
const { execute, canExecute } = useWsQuery();

const isCollapsed = computed(() => uiStore.summarizeCollapsed);

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

// Auto-execute when configuration changes
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
watch(
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
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(() => {
      if (canExecute() && connectionStore.isConnected && datasetStore.hasData) {
        execute();
      }
    }, 300);
  },
  { deep: true },
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
  <div class="border-b border-default">
    <!-- Section Header -->
    <div class="flex items-center justify-between bg-muted/30">
      <button
        class="flex items-center gap-2 px-4 py-2 text-left hover:bg-muted/40 transition-colors"
        @click="uiStore.toggleSection('summarize')"
      >
        <component :is="isCollapsed ? ChevronRight : ChevronDown" class="w-4 h-4 text-muted" />
        <h2 class="text-sm font-medium text-default">Summarize</h2>
        <span v-if="pivotStore.isConfigured" class="text-xs text-muted">
          ({{ pivotStore.valueFields.length }} value{{
            pivotStore.valueFields.length !== 1 ? 's' : ''
          }})
        </span>
      </button>

      <div v-if="!isCollapsed" class="flex items-center gap-3 pr-4">
        <!-- Settings toggles -->
        <label class="flex items-center gap-1.5 text-xs text-muted cursor-pointer" @click.stop>
          <USwitch v-model="pivotStore.showSubtotals" size="xs" />
          Subtotals
        </label>
        <label class="flex items-center gap-1.5 text-xs text-muted cursor-pointer" @click.stop>
          <USwitch v-model="pivotStore.showColumnTotals" size="xs" />
          Totals
        </label>
        <label class="flex items-center gap-1.5 text-xs text-muted cursor-pointer" @click.stop>
          <USwitch v-model="pivotStore.showConditionalFormatting" size="xs" />
          Heatmap
        </label>

        <!-- Decimals -->
        <div class="flex items-center gap-1.5 text-xs text-muted" @click.stop>
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
        <UButton variant="ghost" size="xs" @click.stop="pivotStore.reset()">
          <RotateCcw class="w-3 h-3" />
        </UButton>
      </div>
    </div>

    <!-- Content -->
    <div v-if="!isCollapsed" class="flex bg-muted/10 border-t border-default">
      <!-- Column List (left side) -->
      <div class="w-48 border-r border-default p-3 bg-muted/20">
        <div v-if="columns.length" class="space-y-1 max-h-48 overflow-y-auto">
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
                class="flex items-center gap-2 px-2 py-1.5 rounded-md bg-default/50 hover:bg-default cursor-grab active:cursor-grabbing transition-colors text-xs group"
              >
                <GripVertical class="w-3 h-3 text-muted/30 group-hover:text-muted/60" />
                <component :is="getTypeIcon(element)" class="w-3 h-3 text-muted" />
                <span class="truncate flex-1">{{ element.name }}</span>
              </div>
            </template>
          </draggable>
        </div>

        <div v-else class="text-xs text-muted/60 py-4">No columns loaded</div>
      </div>

      <!-- Buckets (right side) -->
      <div class="flex-1 p-3">
        <!-- Three Bucket Layout -->
        <div class="grid grid-cols-3 gap-3 mb-3">
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
              class="absolute -left-5 top-8 z-10 p-1 rounded-full bg-muted/50 hover:bg-muted text-muted hover:text-default transition-colors"
              title="Flip rows and columns"
              @click="pivotStore.flipRowsAndColumns()"
            >
              <ArrowLeftRight class="w-3 h-3" />
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
        <div class="flex items-center gap-3 pt-2 border-t border-default/50">
          <button
            class="flex items-center gap-1.5 text-xs"
            :class="queryStore.sections.sort.enabled ? 'text-muted' : 'text-muted/50'"
            @click="toggleSort"
          >
            <ArrowUpDown class="w-3.5 h-3.5" />
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
              size="xs"
              @click="toggleSortDirection"
            >
              <component
                :is="queryStore.sortDescending ? ArrowDown : ArrowUp"
                class="w-3.5 h-3.5 mr-1"
              />
              {{ queryStore.sortDescending ? 'DESC' : 'ASC' }}
            </UButton>
          </template>

          <!-- Help text -->
          <div v-if="!pivotStore.isConfigured" class="flex-1 text-right text-xs text-muted/60">
            Drag columns into buckets to build your query
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
