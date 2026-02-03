<script setup>
import { ChevronDown, ChevronRight } from 'lucide-vue-next'

const props = defineProps({
  title: { type: String, required: true },
  enabled: { type: Boolean, default: true },
  collapsed: { type: Boolean, default: false },
  preview: { type: String, default: '' }
})

const emit = defineEmits(['toggle-enable', 'toggle-collapse'])
</script>

<template>
  <div class="border border-default rounded-lg bg-default">
    <!-- Header -->
    <div
      class="flex items-center justify-between px-3 py-2 cursor-pointer hover:bg-muted/50"
      @click="emit('toggle-collapse')"
    >
      <div class="flex items-center gap-2">
        <!-- Collapse toggle -->
        <component
          :is="collapsed ? ChevronRight : ChevronDown"
          class="w-4 h-4 text-muted"
        />

        <!-- Title -->
        <span class="font-medium text-sm" :class="enabled ? 'text-highlighted' : 'text-muted'">
          {{ title }}
        </span>

        <!-- Preview when collapsed -->
        <span v-if="collapsed && preview" class="text-xs text-muted ml-2">
          {{ preview }}
        </span>
      </div>

      <!-- Enable toggle -->
      <div @click.stop>
        <USwitch
          :model-value="enabled"
          size="sm"
          @update:model-value="emit('toggle-enable')"
        />
      </div>
    </div>

    <!-- Content -->
    <div
      v-show="!collapsed && enabled"
      class="px-3 pb-3 border-t border-default"
    >
      <slot />
    </div>
  </div>
</template>
