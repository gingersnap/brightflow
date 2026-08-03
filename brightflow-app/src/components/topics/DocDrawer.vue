<script setup lang="ts">
/**
 * Read-only slideover showing a single document (title, number, monospace
 * body) with a link out to the original. Purely presentational — the parent
 * picks the doc and controls visibility via v-model.
 */

import type { DocRef } from '@/types/generated';

defineProps<{
  modelValue: boolean;
  doc: DocRef | null;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: boolean];
}>();

function close(): void {
  emit('update:modelValue', false);
}
</script>

<template>
  <USlideover :model-value="modelValue" side="right" @update:model-value="close">
    <template #content>
      <div class="flex h-full flex-col">
        <div class="flex items-start justify-between gap-3 border-b border-default p-4">
          <div class="flex flex-col gap-1">
            <span v-if="doc?.number != null" class="text-sm text-muted">#{{ doc.number }}</span>
            <h2 class="text-base font-semibold text-highlighted">
              {{ doc?.title ?? 'Document' }}
            </h2>
          </div>
          <div class="flex items-center gap-1">
            <UButton
              v-if="doc?.htmlUrl"
              :to="doc.htmlUrl"
              target="_blank"
              icon="i-lucide-external-link"
              color="neutral"
              variant="ghost"
              size="md"
              aria-label="Open original"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="md"
              aria-label="Close"
              @click="close"
            />
          </div>
        </div>
        <div class="flex-1 overflow-auto p-4">
          <div v-if="doc?.body" class="font-mono text-sm whitespace-pre-wrap text-default">
            {{ doc.body }}
          </div>
          <div v-else class="text-sm text-muted">No body content.</div>
        </div>
      </div>
    </template>
  </USlideover>
</template>
