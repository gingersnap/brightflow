<script setup lang="ts">
import type { IssueRef } from '@/types/generated';

defineProps<{
  modelValue: boolean;
  issue: IssueRef | null;
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
            <span v-if="issue?.number != null" class="text-sm text-muted">#{{ issue.number }}</span>
            <h2 class="text-base font-semibold text-highlighted">
              {{ issue?.title ?? 'Issue' }}
            </h2>
          </div>
          <UButton
            icon="i-lucide-x"
            color="neutral"
            variant="ghost"
            size="md"
            aria-label="Close"
            @click="close"
          />
        </div>
        <div class="flex-1 overflow-auto p-4">
          <div v-if="issue?.body" class="font-mono text-sm whitespace-pre-wrap text-default">
            {{ issue.body }}
          </div>
          <div v-else class="text-sm text-muted">No body content.</div>
        </div>
      </div>
    </template>
  </USlideover>
</template>
