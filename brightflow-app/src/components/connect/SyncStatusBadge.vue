<script setup lang="ts">
/**
 * Status chip for sync runs: a fixed mapping from the four run states to
 * badge colors, with a pulsing dot while running. Purely presentational —
 * the status arrives as a prop.
 */

defineProps<{
  status: 'pending' | 'running' | 'completed' | 'failed';
}>();

const STATUS_COLORS = {
  completed: 'success',
  failed: 'error',
  running: 'info',
  pending: 'neutral',
} as const;
</script>

<template>
  <UBadge :color="STATUS_COLORS[status]" variant="subtle" size="md">
    <template #leading>
      <span
        class="size-1.5 rounded-full bg-current"
        :class="{ 'animate-pulse': status === 'running' }"
      />
    </template>
    {{ status }}
  </UBadge>
</template>
