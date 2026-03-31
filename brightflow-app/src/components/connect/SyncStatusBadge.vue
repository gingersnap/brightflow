<script setup lang="ts">
defineProps<{
  status: 'pending' | 'running' | 'completed' | 'failed';
}>();
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium"
    :class="{
      'bg-green-500/10 text-green-500': status === 'completed',
      'bg-red-500/10 text-red-500': status === 'failed',
      'bg-blue-500/10 text-blue-500': status === 'running',
      'bg-neutral-500/10 text-neutral-400': status === 'pending',
    }"
  >
    <span
      class="h-1.5 w-1.5 rounded-full"
      :class="{
        'bg-green-500': status === 'completed',
        'bg-red-500': status === 'failed',
        'animate-pulse bg-blue-500': status === 'running',
        'bg-neutral-400': status === 'pending',
      }"
    />
    {{ status }}
  </span>
</template>
