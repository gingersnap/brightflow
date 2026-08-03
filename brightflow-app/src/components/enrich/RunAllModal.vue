<script setup lang="ts">
/**
 * Confirmation modal for a full enrichment run. The point is informed
 * consent: picking a scope (missing / all / failed) refetches a row and
 * token estimate so the cost is visible before Run is clicked, along with
 * whether the estimate comes from token history or just prompt length.
 */

import { ref, watch } from 'vue';

import { enrichFnApi } from '@/services/api';
import type { EnrichEstimate, RunScope } from '@/types/enrichment';

const props = defineProps<{
  open: boolean;
  functionId: string;
  initialScope?: RunScope;
}>();

const emit = defineEmits<{
  'update:open': [open: boolean];
  confirm: [scope: RunScope];
}>();

const scope = ref<RunScope>(props.initialScope ?? 'missing');
const estimate = ref<EnrichEstimate | null>(null);
const loading = ref(false);

const scopeItems = [
  { label: 'Rows without a result yet', value: 'missing' },
  { label: 'All rows (recompute cached)', value: 'all' },
  { label: 'Previously failed rows', value: 'failed' },
];

watch(
  () => props.open,
  (open) => {
    if (open) {
      scope.value = props.initialScope ?? 'missing';
    }
  },
);

watch(
  () => [props.open, scope.value] as const,
  async ([open, currentScope]) => {
    if (!open) {
      return;
    }
    loading.value = true;
    estimate.value = null;
    try {
      estimate.value = await enrichFnApi.estimate(props.functionId, currentScope);
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <UModal :open="open" title="Run enrichment" @update:open="emit('update:open', $event)">
    <template #body>
      <div class="space-y-3">
        <div>
          <p class="mb-1 text-sm font-medium text-highlighted">Rows to run</p>
          <USelect v-model="scope" :items="scopeItems" size="md" class="w-full" />
        </div>
        <div v-if="loading" class="flex items-center gap-2 text-sm text-muted">
          <UIcon name="i-lucide-loader-circle" class="size-4 animate-spin" />
          Estimating…
        </div>
        <div v-else-if="estimate" class="space-y-1 rounded-lg border border-default p-3">
          <p class="text-sm text-default">
            <span class="font-medium text-highlighted">
              {{ estimate.rowsToRun.toLocaleString() }}
            </span>
            of {{ estimate.rowsTotal.toLocaleString() }} rows will be computed
            <template v-if="estimate.rowsToRun < estimate.rowsTotal">
              (the rest are cached)
            </template>
          </p>
          <p class="text-sm text-default">
            Estimated tokens:
            <span class="font-medium text-highlighted">
              ~{{ estimate.estimatedTokens.toLocaleString() }}
            </span>
          </p>
          <p class="text-sm text-muted">
            Approximate — based on the
            {{
              estimate.basis === 'history'
                ? '75th-percentile token use of previous rows'
                : 'prompt length (no history yet)'
            }}.
          </p>
        </div>
      </div>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton size="md" color="neutral" variant="ghost" @click="emit('update:open', false)">
          Cancel
        </UButton>
        <UButton size="md" color="primary" @click="emit('confirm', scope)">
          Run {{ estimate ? `~${estimate.rowsToRun.toLocaleString()} rows` : '' }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>
