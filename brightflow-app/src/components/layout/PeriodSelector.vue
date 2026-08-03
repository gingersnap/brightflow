<script setup lang="ts">
/**
 * Time-period picker rendered as tabs. Ships a default preset list
 * (today through 12 months) so most call sites only bind v-model, and
 * coerces the tab value to a string to keep the v-model contract.
 */

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
  <UTabs
    :model-value="modelValue"
    :items="periods"
    :content="false"
    size="md"
    @update:model-value="(v) => emit('update:modelValue', String(v))"
  />
</template>
