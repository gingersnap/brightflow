<script setup>
import { useQueryStore } from '@/stores/query'

const queryStore = useQueryStore()

const presets = [100, 500, 1000, 5000, 10000]
</script>

<template>
  <div class="pt-3 space-y-3">
    <!-- Limit input -->
    <div>
      <label class="text-xs font-medium text-muted mb-2 block">Row Limit</label>
      <UInput
        v-model.number="queryStore.limit"
        type="number"
        min="1"
        max="100000"
        placeholder="Number of rows"
        class="w-40"
      />
    </div>

    <!-- Quick presets -->
    <div class="flex items-center gap-2">
      <span class="text-xs text-muted">Quick:</span>
      <UButton
        v-for="preset in presets"
        :key="preset"
        variant="ghost"
        color="neutral"
        size="xs"
        :class="{ 'bg-muted': queryStore.limit === preset }"
        @click="queryStore.limit = preset"
      >
        {{ preset.toLocaleString() }}
      </UButton>
    </div>
  </div>
</template>
