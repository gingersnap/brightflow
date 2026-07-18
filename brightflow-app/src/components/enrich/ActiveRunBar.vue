<script setup lang="ts">
import { computed } from 'vue';

import type { EnrichRun } from '@/types/enrichment';

const props = defineProps<{
  run: EnrichRun;
}>();

defineEmits<{
  cancel: [];
  rerunFailed: [];
  dismiss: [];
}>();

const progress = computed(() => {
  if (props.run.rowsTotal <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((props.run.rowsDone / props.run.rowsTotal) * 100));
});

const isRunning = computed(() => props.run.status === 'running');
</script>

<template>
  <div class="space-y-2 rounded-lg border border-default bg-elevated p-3">
    <div class="flex items-center gap-2">
      <UIcon
        v-if="isRunning"
        name="i-lucide-loader-circle"
        class="size-4 animate-spin text-primary-500"
      />
      <UIcon
        v-else-if="run.status === 'completed'"
        name="i-lucide-check"
        class="size-4 text-success"
      />
      <UIcon v-else name="i-lucide-circle-alert" class="size-4 text-warning" />
      <p class="text-sm font-medium text-highlighted">
        {{
          isRunning
            ? `Running — ${run.rowsDone.toLocaleString()} / ${run.rowsTotal.toLocaleString()} rows`
            : `Run ${run.status} — ${run.rowsDone.toLocaleString()} rows, ${run.rowsFailed.toLocaleString()} failed, ${run.rowsCached.toLocaleString()} cached`
        }}
      </p>
      <span class="ml-auto text-sm text-muted">
        {{ run.totalTokens.toLocaleString() }} tokens
      </span>
      <UButton v-if="isRunning" size="xs" color="neutral" variant="ghost" @click="$emit('cancel')">
        Cancel
      </UButton>
      <UButton
        v-else
        size="xs"
        color="neutral"
        variant="ghost"
        icon="i-lucide-x"
        aria-label="Dismiss"
        @click="$emit('dismiss')"
      />
    </div>
    <UProgress v-if="isRunning" :model-value="progress" size="sm" />
    <p v-if="run.error" class="text-sm text-error">{{ run.error }}</p>
    <UButton
      v-if="!isRunning && run.rowsFailed > 0"
      size="md"
      color="neutral"
      variant="soft"
      icon="i-lucide-rotate-ccw"
      @click="$emit('rerunFailed')"
    >
      Re-run {{ run.rowsFailed.toLocaleString() }} failed rows
    </UButton>
  </div>
</template>
