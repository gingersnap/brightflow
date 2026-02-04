<script setup lang="ts">
import { ref, watch, type Component } from 'vue';
import { X, Hash, Type, HelpCircle, GripVertical } from 'lucide-vue-next';
import draggable from 'vuedraggable';
import type { PivotField } from '@/types';

interface AggregationOption {
  value: string;
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
    fields: () => [],
    showAggregation: false,
    maxItems: null,
    disabled: false,
    disabledMessage: 'Not available',
    aggregations: () => [
      { value: 'count', label: 'Count' },
      { value: 'sum', label: 'Sum' },
      { value: 'avg', label: 'Average' },
      { value: 'min', label: 'Min' },
      { value: 'max', label: 'Max' },
      { value: 'median', label: 'Median' },
    ],
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
  if (props.disabled) return false;
  if (props.maxItems === null) return true;
  return props.fields.length < props.maxItems;
}

// Get icon for field type
function getTypeIcon(field: PivotField): Component {
  const dtype = field.dtype;
  if (['int', 'float', 'decimal', 'number'].includes(dtype)) return Hash;
  if (['string', 'text', 'varchar'].includes(dtype)) return Type;
  return HelpCircle;
}

// Handle all drag changes - differentiates between add and reorder
function handleChange(evt: DragEvent): void {
  if (evt.added) {
    // Item was cloned from sidebar
    // Remove it from local list (vuedraggable added it) - store will add properly
    const addedIndex = evt.added.newIndex;
    localFields.value.splice(addedIndex, 1);

    if (!canAcceptMore()) return;

    const addedElement = evt.added.element;
    if (addedElement) {
      const column = addedElement.column ?? addedElement.name;
      const dtype = addedElement.dtype;

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
  <div class="flex flex-col h-full min-h-[120px]">
    <!-- Header -->
    <div class="text-xs font-semibold text-muted uppercase tracking-wide mb-2">
      {{ title }}
    </div>

    <!-- Dropzone -->
    <draggable
      :list="localFields"
      :group="{ name: 'columns', pull: false, put: canAcceptMore() }"
      item-key="id"
      class="flex-1 min-h-[80px] rounded-md border-2 border-dashed p-2 transition-colors"
      :class="{
        'border-primary bg-primary/5': canAcceptMore(),
        'border-muted/30 bg-muted/5 opacity-50': disabled,
        'border-muted/50 bg-muted/10': !canAcceptMore() && !disabled
      }"
      ghost-class="opacity-50"
      drag-class="bg-primary/20"
      @change="handleChange"
    >
      <template #item="{ element }">
        <div
          class="flex items-center gap-2 px-2 py-1.5 mb-1 rounded-md bg-default border border-default hover:border-primary/50 cursor-grab active:cursor-grabbing transition-colors group"
        >
          <GripVertical class="w-3 h-3 text-muted/50" />
          <component
            :is="getTypeIcon(element)"
            class="w-3.5 h-3.5 text-muted flex-shrink-0"
          />
          <span class="text-sm text-default truncate flex-1">
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
            @update:model-value="(val: string) => emit('update', element.id, { aggregation: val })"
          />

          <!-- Remove button -->
          <button
            class="p-0.5 rounded hover:bg-muted transition-colors opacity-0 group-hover:opacity-100"
            @click.stop="emit('remove', element.id)"
          >
            <X class="w-3.5 h-3.5 text-muted hover:text-default" />
          </button>
        </div>
      </template>

      <!-- Empty state -->
      <template #footer>
        <div
          v-if="localFields.length === 0"
          class="flex items-center justify-center h-full text-xs py-4"
          :class="disabled ? 'text-muted/50' : 'text-muted/70'"
        >
          {{ disabled ? disabledMessage : 'Drop columns here' }}
        </div>
      </template>
    </draggable>
  </div>
</template>
