<script setup lang="ts">
/**
 * One drag-target bucket (rows / columns / values) of the pivot builder.
 * Works on a local copy of the fields so vuedraggable never mutates props:
 * a dropped column is stripped back out of the local list and re-emitted as
 * an `add` event for the store to apply, and reorders are emitted whole.
 */

import { ref, watch } from 'vue';
import draggable from 'vuedraggable';

import type { AggFn, PivotField } from '@/types';
import { isNumericDtype, isStringDtype } from '@/utils/dtype';

// AggFn (not string) so USelectMenu emits a value assignable to PivotField.aggregation.
interface AggregationOption {
  value: AggFn;
  label: string;
}

interface DragElement {
  column?: string;
  name?: string;
  dtype?: string;
}

interface DragEvent {
  added?: {
    newIndex: number;
    element: DragElement;
  };
  moved?: {
    oldIndex: number;
    newIndex: number;
  };
}

const props = withDefaults(
  defineProps<{
    title: string;
    fields: PivotField[];
    bucket: 'rows' | 'columns' | 'values';
    showAggregation?: boolean;
    maxItems?: number | null;
    disabled?: boolean;
    disabledMessage?: string;
    aggregations?: AggregationOption[];
  }>(),
  {
    aggregations: () => [
      { value: 'count', label: 'Count' },
      { value: 'sum', label: 'Sum' },
      { value: 'avg', label: 'Average' },
      { value: 'min', label: 'Min' },
      { value: 'max', label: 'Max' },
      { value: 'median', label: 'Median' },
    ],
    disabled: false,
    disabledMessage: 'Not available',
    fields: () => [],
    maxItems: null,
    showAggregation: false,
  },
);

const emit = defineEmits<{
  add: [field: { column: string; dtype: string }];
  remove: [id: string];
  reorder: [fields: PivotField[]];
  update: [id: string, updates: Partial<PivotField>];
}>();

// Local copy of fields for draggable - synced from props
// This prevents vuedraggable from mutating props directly
const localFields = ref<PivotField[]>([...props.fields]);

// Sync local fields when props change (from store updates)
watch(
  () => props.fields,
  (newFields) => {
    localFields.value = [...newFields];
  },
  { deep: true },
);

// Whether we can accept more items
function canAcceptMore(): boolean {
  if (props.disabled) {
    return false;
  }
  if (props.maxItems === null) {
    return true;
  }
  return props.fields.length < props.maxItems;
}

// Get icon for field type
function getTypeIcon(field: PivotField): string {
  const { dtype } = field;
  if (isNumericDtype(dtype)) {
    return 'i-lucide-hash';
  }
  if (isStringDtype(dtype)) {
    return 'i-lucide-type';
  }
  return 'i-lucide-circle-help';
}

// Handle all drag changes - differentiates between add and reorder
function handleChange(evt: DragEvent): void {
  if (evt.added) {
    // Item was cloned from sidebar
    // Remove it from local list (vuedraggable added it) - store will add properly
    const addedIndex = evt.added.newIndex;
    localFields.value.splice(addedIndex, 1);

    if (!canAcceptMore()) {
      return;
    }

    const addedElement = evt.added.element;
    if (addedElement) {
      const column = addedElement.column ?? addedElement.name;
      const { dtype } = addedElement;

      if (column && dtype) {
        emit('add', { column, dtype });
      }
    }
  } else if (evt.moved) {
    // Reorder within same bucket - emit new order
    emit('reorder', [...localFields.value]);
  }
}
</script>

<template>
  <div class="flex h-full min-h-[120px] flex-col">
    <!-- Header -->
    <div class="mb-2 text-xs font-semibold tracking-wide text-muted uppercase">
      {{ title }}
    </div>

    <!-- Dropzone -->
    <draggable
      :list="localFields"
      :group="{ name: 'columns', pull: false, put: canAcceptMore() }"
      item-key="id"
      class="min-h-[80px] flex-1 rounded-md border-2 border-dashed p-2 transition-colors"
      :class="{
        'border-primary bg-primary/5': canAcceptMore(),
        'border-muted/30 bg-muted/5 opacity-50': disabled,
        'border-muted/50 bg-muted/10': !canAcceptMore() && !disabled,
      }"
      ghost-class="opacity-50"
      drag-class="bg-primary/20"
      @change="handleChange"
    >
      <template #item="{ element }">
        <div
          class="group mb-1 flex cursor-grab items-center gap-2 rounded-md border border-default bg-default px-2 py-1.5 transition-colors hover:border-primary/50 active:cursor-grabbing"
        >
          <UIcon name="i-lucide-grip-vertical" class="h-3 w-3 text-muted/50" />
          <UIcon :name="getTypeIcon(element)" class="h-3.5 w-3.5 flex-shrink-0 text-muted" />
          <span class="flex-1 truncate text-sm text-default">
            {{ element.column }}
          </span>

          <!-- Aggregation selector for values bucket -->
          <USelectMenu
            v-if="showAggregation"
            :model-value="element.aggregation"
            :items="aggregations"
            value-key="value"
            size="xs"
            class="w-20"
            @update:model-value="(val: AggFn) => emit('update', element.id, { aggregation: val })"
          />

          <!-- Remove button -->
          <button
            class="rounded p-0.5 opacity-0 transition-colors group-hover:opacity-100 hover:bg-muted"
            @click.stop="emit('remove', element.id)"
          >
            <UIcon name="i-lucide-x" class="h-3.5 w-3.5 text-muted hover:text-default" />
          </button>
        </div>
      </template>

      <!-- Empty state -->
      <template #footer>
        <div
          v-if="localFields.length === 0"
          class="flex h-full items-center justify-center py-4 text-sm"
          :class="disabled ? 'text-muted/50' : 'text-muted/70'"
        >
          {{ disabled ? disabledMessage : 'Drop columns here' }}
        </div>
      </template>
    </draggable>
  </div>
</template>
