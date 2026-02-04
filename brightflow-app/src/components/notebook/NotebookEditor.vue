<script setup lang="ts">
import { ref, computed } from 'vue'
import { Database, ChevronDown, ChevronUp } from 'lucide-vue-next'
import { useDatasetStore } from '@/stores/dataset'

const datasetStore = useDatasetStore()
const collapsed = ref(false)

// Step definitions - will be expanded later for full notebook editor
const steps = computed(() => [
  {
    id: 'data',
    label: datasetStore.name || 'Select Data',
    icon: Database,
    active: true,
    preview: datasetStore.name ? `${datasetStore.rowCount?.toLocaleString()} rows` : null
  }
])
</script>

<template>
  <div class="border-b border-default bg-muted/20">
    <!-- Notebook Header -->
    <div class="flex items-center justify-between px-4 py-2">
      <button
        class="flex items-center gap-2 text-sm font-medium text-muted hover:text-default transition-colors"
        @click="collapsed = !collapsed"
      >
        <component :is="collapsed ? ChevronDown : ChevronUp" class="w-4 h-4" />
        Query Builder
      </button>
    </div>

    <!-- Notebook Steps (collapsible) -->
    <div
      v-show="!collapsed"
      class="px-4 py-3 border-t border-default/50"
    >
      <div class="flex items-center gap-2 flex-wrap">
        <!-- Data Step -->
        <div
          v-for="step in steps"
          :key="step.id"
          class="flex items-center"
        >
          <div
            class="flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors"
            :class="step.active ? 'bg-primary/10 text-primary' : 'bg-muted/50 text-muted'"
          >
            <component :is="step.icon" class="w-4 h-4" />
            <span class="text-sm font-medium">{{ step.label }}</span>
            <span v-if="step.preview" class="text-xs opacity-70">
              ({{ step.preview }})
            </span>
          </div>
        </div>

        <!-- Future: Filter, Summarize, Sort steps will go here -->
      </div>
    </div>
  </div>
</template>
