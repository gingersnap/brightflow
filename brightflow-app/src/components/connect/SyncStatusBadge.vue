<script setup lang="ts">
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
