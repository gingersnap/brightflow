<script setup>
import { Plus, X } from 'lucide-vue-next'
import { computed } from 'vue'
import { useQueryStore } from '@/stores/query'
import { useDatasetStore } from '@/stores/dataset'
import { useAggregations } from '@/composables/useAggregations'

const queryStore = useQueryStore()
const datasetStore = useDatasetStore()
const { getAllAggregations } = useAggregations()

const columnItems = computed(() =>
  datasetStore.columns.map(col => col.name)
)

const aggColumnItems = computed(() => [
  '* (all rows)',
  ...datasetStore.columns.map(col => col.name)
])

const aggregationItems = computed(() =>
  getAllAggregations().map(agg => ({
    label: agg.label,
    value: agg.value
  }))
)
</script>

<template>
  <div class="pt-3 space-y-4">
    <!-- Group by columns -->
    <div>
      <label class="text-xs font-medium text-muted mb-2 block">Group By Columns</label>
      <USelectMenu
        v-model="queryStore.groupByColumns"
        :items="columnItems"
        placeholder="Select columns to group by"
        multiple
      />
    </div>

    <!-- Aggregations -->
    <div>
      <div class="flex items-center justify-between mb-2">
        <label class="text-xs font-medium text-muted">Aggregations</label>
        <UButton
          variant="ghost"
          color="neutral"
          size="xs"
          @click="queryStore.addAggregation"
        >
          <Plus class="w-3 h-3 mr-1" />
          Add
        </UButton>
      </div>

      <div v-if="queryStore.aggregations.length === 0" class="text-sm text-muted py-2">
        No aggregations. Add at least one when grouping.
      </div>

      <div class="space-y-2">
        <div
          v-for="agg in queryStore.aggregations"
          :key="agg.id"
          class="flex items-center gap-2 p-2 bg-muted/30 rounded-lg"
        >
          <!-- Function -->
          <USelectMenu
            :model-value="agg.function"
            :items="aggregationItems"
            value-key="value"
            placeholder="Function"
            class="w-32"
            @update:model-value="(val) => queryStore.updateAggregation(agg.id, { function: val })"
          />

          <!-- Column -->
          <USelectMenu
            :model-value="agg.column"
            :items="aggColumnItems"
            placeholder="Column"
            class="w-40"
            @update:model-value="(val) => queryStore.updateAggregation(agg.id, { column: val === '* (all rows)' ? '*' : val })"
          />

          <!-- Alias -->
          <UInput
            :model-value="agg.alias"
            placeholder="Alias (optional)"
            class="flex-1"
            @update:model-value="(val) => queryStore.updateAggregation(agg.id, { alias: val })"
          />

          <!-- Remove -->
          <UButton
            variant="ghost"
            color="neutral"
            size="xs"
            @click="queryStore.removeAggregation(agg.id)"
          >
            <X class="w-4 h-4" />
          </UButton>
        </div>
      </div>
    </div>
  </div>
</template>
