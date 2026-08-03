<script setup lang="ts">
/**
 * V-model select for an output column's dtype. Enum is the one structured
 * case: choosing it reveals a second input where the allowed labels are
 * typed as comma-separated text and parsed into the values array on every
 * edit.
 */

import { computed } from 'vue';

import type { OutputType } from '@/types/enrichment';

const props = defineProps<{
  modelValue: OutputType;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: OutputType];
}>();

const typeItems = [
  { label: 'Text', value: 'string' },
  { label: 'Number', value: 'number' },
  { label: 'Yes / No', value: 'bool' },
  { label: 'JSON', value: 'json' },
  { label: 'Labels (enum)', value: 'enum' },
];

const selectedType = computed({
  get: () => props.modelValue.type,
  set: (type: string) => {
    if (type === 'enum') {
      const existing = props.modelValue.type === 'enum' ? props.modelValue.values : [];
      emit('update:modelValue', { type: 'enum', values: existing });
    } else {
      emit('update:modelValue', { type } as OutputType);
    }
  },
});

const enumText = computed({
  get: () => (props.modelValue.type === 'enum' ? props.modelValue.values.join(', ') : ''),
  set: (text: string) => {
    emit('update:modelValue', {
      type: 'enum',
      values: text
        .split(',')
        .map((v) => v.trim())
        .filter((v) => v !== ''),
    });
  },
});
</script>

<template>
  <div class="flex items-center gap-2">
    <USelect v-model="selectedType" :items="typeItems" size="md" class="w-36" />
    <UInput
      v-if="modelValue.type === 'enum'"
      v-model="enumText"
      placeholder="positive, negative, neutral"
      size="md"
      class="flex-1"
    />
  </div>
</template>
