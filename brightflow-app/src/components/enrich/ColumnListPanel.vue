<script setup lang="ts">
import { computed } from 'vue';

import type { EnrichFunction, FunctionKind } from '@/types/enrichment';

import FunctionBadge from './FunctionBadge.vue';

const props = defineProps<{
  sourceColumns: { name: string; dtype: string }[];
  functions: EnrichFunction[];
  selectedId: string | null;
}>();

const emit = defineEmits<{
  select: [id: string];
  create: [kind: FunctionKind];
}>();

const hasTopicModel = computed(() => props.functions.some((f) => f.kind === 'topic_model'));

/** Columns owned by functions (outputs + status) — shown in the derived group. */
const derivedColumnNames = computed(() => {
  const names = new Set<string>();
  for (const fn of props.functions) {
    names.add(`${fn.name}__status`);
    const config = fn.config as { outputs?: { name: string }[] } | null;
    for (const output of config?.outputs ?? []) {
      names.add(output.name);
    }
  }
  return names;
});

const plainSourceColumns = computed(() =>
  props.sourceColumns.filter((c) => !derivedColumnNames.value.has(c.name)),
);

const addItems = computed(() => [
  [
    {
      icon: 'i-lucide-wand-sparkles',
      label: 'AI prompt',
      onSelect: () => emit('create', 'llm_prompt'),
    },
    {
      disabled: hasTopicModel.value,
      icon: 'i-lucide-shapes',
      label: hasTopicModel.value ? 'Topic model (already exists)' : 'Topic model',
      onSelect: () => emit('create', 'topic_model'),
    },
    {
      disabled: true,
      icon: 'i-lucide-tags',
      label: 'Classifier (coming soon)',
    },
  ],
]);
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto border-r border-default">
    <!-- Derived columns -->
    <div class="space-y-1 p-3">
      <div class="flex items-center justify-between">
        <p class="text-xs font-medium tracking-wider text-muted uppercase">Derived columns</p>
        <UDropdownMenu :items="addItems">
          <UButton size="xs" color="primary" variant="soft" icon="i-lucide-plus">Add</UButton>
        </UDropdownMenu>
      </div>
      <button
        v-for="fn in functions"
        :key="fn.id"
        class="flex w-full cursor-pointer flex-col gap-1 rounded-lg border p-2.5 text-left transition-colors"
        :class="
          fn.id === selectedId
            ? 'border-primary-500 bg-primary-500/5'
            : 'border-default hover:border-primary-500/50'
        "
        @click="emit('select', fn.id)"
      >
        <p class="text-sm font-medium text-highlighted">{{ fn.name }}</p>
        <FunctionBadge :fn="fn" />
      </button>
      <p v-if="functions.length === 0" class="py-2 text-sm text-muted">
        No derived columns yet. Add one to enrich this table.
      </p>
    </div>

    <!-- Source columns -->
    <div class="space-y-1 border-t border-default p-3">
      <p class="text-xs font-medium tracking-wider text-muted uppercase">Source columns</p>
      <div
        v-for="col in plainSourceColumns"
        :key="col.name"
        class="flex items-center gap-2 rounded px-2 py-1"
      >
        <UIcon name="i-lucide-columns-3" class="size-3.5 shrink-0 text-muted" />
        <span class="truncate text-sm text-default">{{ col.name }}</span>
        <span class="ml-auto text-xs text-muted">{{ col.dtype }}</span>
      </div>
      <p v-if="plainSourceColumns.length === 0" class="py-2 text-sm text-muted">
        No columns found.
      </p>
    </div>
  </div>
</template>
