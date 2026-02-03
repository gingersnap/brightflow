<script setup>
import { X, Hash, Type, ToggleLeft } from 'lucide-vue-next'
import { computed, watch } from 'vue'
import { useDatasetStore } from '@/stores/dataset'
import { useOperators } from '@/composables/useOperators'

const props = defineProps({
  filter: { type: Object, required: true }
})

const emit = defineEmits(['update', 'remove'])

const datasetStore = useDatasetStore()
const { getOperatorsForType, operatorNeedsValue, getDefaultOperator } = useOperators()

// Column items for select - Nuxt UI uses 'items' with 'label' for display
const columnItems = computed(() =>
  datasetStore.columns.map(col => ({
    label: col.name,
    value: col.name,
    dtype: col.dtype
  }))
)

// Get available operators for selected column
const operatorItems = computed(() => {
  if (!props.filter.column) return []
  const column = datasetStore.getColumnByName(props.filter.column)
  return getOperatorsForType(column?.dtype)
})

// Check if current operator needs value
const needsValue = computed(() => operatorNeedsValue(props.filter.op))

// When column changes, update operator to appropriate default
watch(() => props.filter.column, (newColumn) => {
  if (newColumn) {
    const column = datasetStore.getColumnByName(newColumn)
    const defaultOp = getDefaultOperator(column?.dtype)
    emit('update', { op: defaultOp, value: null })
  }
})
</script>

<template>
  <div class="flex items-center gap-2 p-2 bg-muted/30 rounded-lg">
    <!-- Column select -->
    <USelectMenu
      :model-value="filter.column"
      :items="columnItems"
      value-key="value"
      placeholder="Select column"
      class="w-48"
      @update:model-value="(val) => emit('update', { column: val })"
    />

    <!-- Operator select -->
    <USelectMenu
      :model-value="filter.op"
      :items="operatorItems"
      value-key="value"
      placeholder="Operator"
      :disabled="!filter.column"
      class="w-40"
      @update:model-value="(val) => emit('update', { op: val, value: null })"
    />

    <!-- Value input -->
    <UInput
      v-if="needsValue"
      :model-value="filter.value"
      placeholder="Value"
      :disabled="!filter.column || !filter.op"
      class="flex-1"
      @update:model-value="(val) => emit('update', { value: val })"
    />

    <!-- Remove button -->
    <UButton
      variant="ghost"
      color="neutral"
      size="xs"
      @click="emit('remove')"
    >
      <X class="w-4 h-4" />
    </UButton>
  </div>
</template>
