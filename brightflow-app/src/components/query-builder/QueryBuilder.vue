<script setup lang="ts">
import { useQuery } from '@/composables/useQuery'
import { useQueryStore } from '@/stores/query'
import { useResultsStore } from '@/stores/results'
import QuerySection from './QuerySection.vue'
import FilterSection from './FilterSection.vue'
import GroupBySection from './GroupBySection.vue'
import SortSection from './SortSection.vue'
import LimitSection from './LimitSection.vue'

const queryStore = useQueryStore()
const resultsStore = useResultsStore()
const { execute, canExecute } = useQuery()

function handleRun() {
  execute()
}

function handleReset() {
  queryStore.reset()
  resultsStore.clear()
}

// Keyboard shortcut: Cmd/Ctrl + Enter to run
function handleKeydown(event: KeyboardEvent): void {
  if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
    event.preventDefault()
    if (canExecute()) {
      handleRun()
    }
  }
}
</script>

<template>
  <div class="p-4 bg-default" @keydown="handleKeydown" tabindex="0">
    <!-- Header with Run button -->
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold text-highlighted">Query Builder</h2>
      <div class="flex items-center gap-2">
        <UButton
          label="Reset"
          icon="i-lucide-rotate-ccw"
          variant="ghost"
          color="neutral"
          size="sm"
          @click="handleReset"
        />
        <UButton
          label="Run Query"
          icon="i-lucide-play"
          variant="solid"
          color="primary"
          size="sm"
          @click="handleRun"
        />
      </div>
    </div>

    <!-- Query Sections -->
    <div class="space-y-2">
      <!-- Filter Section -->
      <QuerySection
        title="Filters"
        :enabled="queryStore.sections.filter.enabled"
        :collapsed="queryStore.sections.filter.collapsed"
        :preview="queryStore.previewTexts.filter"
        @toggle-enable="queryStore.toggleSection('filter')"
        @toggle-collapse="queryStore.toggleCollapse('filter')"
      >
        <FilterSection />
      </QuerySection>

      <!-- Group By Section -->
      <QuerySection
        title="Group By"
        :enabled="queryStore.sections.groupBy.enabled"
        :collapsed="queryStore.sections.groupBy.collapsed"
        :preview="queryStore.previewTexts.groupBy"
        @toggle-enable="queryStore.toggleSection('groupBy')"
        @toggle-collapse="queryStore.toggleCollapse('groupBy')"
      >
        <GroupBySection />
      </QuerySection>

      <!-- Sort Section -->
      <QuerySection
        title="Sort"
        :enabled="queryStore.sections.sort.enabled"
        :collapsed="queryStore.sections.sort.collapsed"
        :preview="queryStore.previewTexts.sort"
        @toggle-enable="queryStore.toggleSection('sort')"
        @toggle-collapse="queryStore.toggleCollapse('sort')"
      >
        <SortSection />
      </QuerySection>

      <!-- Limit Section -->
      <QuerySection
        title="Limit"
        :enabled="queryStore.sections.limit.enabled"
        :collapsed="queryStore.sections.limit.collapsed"
        :preview="queryStore.previewTexts.limit"
        @toggle-enable="queryStore.toggleSection('limit')"
        @toggle-collapse="queryStore.toggleCollapse('limit')"
      >
        <LimitSection />
      </QuerySection>
    </div>
  </div>
</template>
