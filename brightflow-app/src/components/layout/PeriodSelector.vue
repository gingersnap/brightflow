<script setup lang="ts">
interface PeriodOption {
  label: string;
  value: string;
}

withDefaults(
  defineProps<{
    modelValue: string;
    periods?: PeriodOption[];
  }>(),
  {
    periods: () => [
      { label: 'Today', value: 'today' },
      { label: '7 days', value: '7d' },
      { label: '30 days', value: '30d' },
      { label: 'This month', value: 'month' },
      { label: '12 months', value: '12m' },
    ],
  },
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();
</script>

<template>
  <div class="flex items-center gap-1 rounded-lg bg-elevated p-0.5">
    <button
      v-for="p in periods"
      :key="p.value"
      class="rounded-md px-3 py-1 text-sm font-medium transition-colors"
      :class="
        modelValue === p.value
          ? 'bg-default text-highlighted shadow-sm'
          : 'cursor-pointer text-muted hover:text-highlighted'
      "
      @click="emit('update:modelValue', p.value)"
    >
      {{ p.label }}
    </button>
  </div>
</template>
