<script setup lang="ts">
import { computed, ref } from 'vue';

import { extractColumnRefs, insertColumnRef } from './promptTokens';

const props = defineProps<{
  modelValue: string;
  /** Columns available on the table (source + other functions' outputs). */
  columns: string[];
}>();

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const textareaWrap = ref<HTMLElement | null>(null);

const template = computed({
  get: () => props.modelValue,
  set: (value: string) => emit('update:modelValue', value),
});

const refs = computed(() => extractColumnRefs(props.modelValue));
const missingRefs = computed(() => refs.value.filter((r) => !props.columns.includes(r)));

const insertItems = computed(() => [
  props.columns.map((column) => ({
    label: column,
    icon: 'i-lucide-columns-3',
    onSelect: () => insertColumn(column),
  })),
]);

function insertColumn(column: string): void {
  const textarea = textareaWrap.value?.querySelector('textarea');
  const cursor = textarea?.selectionStart ?? props.modelValue.length;
  const { text, cursor: nextCursor } = insertColumnRef(props.modelValue, cursor, column);
  emit('update:modelValue', text);
  requestAnimationFrame(() => {
    textarea?.focus();
    textarea?.setSelectionRange(nextCursor, nextCursor);
  });
}
</script>

<template>
  <div class="space-y-2">
    <div class="flex items-center justify-between">
      <p class="text-sm font-medium text-highlighted">Prompt</p>
      <UDropdownMenu :items="insertItems">
        <UButton size="xs" color="neutral" variant="soft" icon="i-lucide-braces">
          Insert column
        </UButton>
      </UDropdownMenu>
    </div>
    <div ref="textareaWrap">
      <UTextarea
        v-model="template"
        :rows="6"
        autoresize
        placeholder="Classify the sentiment of {{col:feedback}} as positive or negative."
        class="w-full font-mono"
      />
    </div>
    <!-- Referenced-column chip strip with missing-column warnings -->
    <div v-if="refs.length > 0" class="flex flex-wrap items-center gap-1.5">
      <UBadge
        v-for="r in refs"
        :key="r"
        :color="missingRefs.includes(r) ? 'error' : 'neutral'"
        variant="subtle"
        size="sm"
        :icon="missingRefs.includes(r) ? 'i-lucide-triangle-alert' : 'i-lucide-columns-3'"
      >
        {{ r }}
      </UBadge>
      <span v-if="missingRefs.length > 0" class="text-sm text-error">
        {{ missingRefs.length === 1 ? 'Column not found' : 'Columns not found' }} in this table
      </span>
    </div>
    <p v-else class="text-sm text-muted">
      Reference row values with <code class="font-mono">&lcub;&lcub;col:Name&rcub;&rcub;</code> —
      use "Insert column".
    </p>
  </div>
</template>
