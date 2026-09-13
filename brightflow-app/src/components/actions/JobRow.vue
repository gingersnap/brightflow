<script setup lang="ts">
/**
 * One background job as a row: in the Running jobs tab while it runs, and
 * in the log once it finished. Status is drawn as well as written — a
 * spinner while running, a coloured word after — and the row cancels the
 * job when its kind allows it, through that kind's own endpoint.
 */

import { computed, ref } from 'vue';

import { agentApi, enrichFnApi } from '@/services/api';
import type { Job } from '@/types/generated';

const props = defineProps<{ job: Job }>();
const emit = defineEmits<{ cancelled: [] }>();

const cancelling = ref(false);

const KIND_ICONS: Record<Job['kind'], string> = {
  agent_run: 'i-lucide-bot',
  connector_sync: 'i-lucide-cable',
  enrichment_run: 'i-lucide-messages-square',
  insight_run: 'i-lucide-sparkles',
  model_build: 'i-lucide-layers',
};

const statusColor: Record<string, string> = {
  cancelled: 'text-muted',
  completed: 'text-emerald-500',
  failed: 'text-red-500',
  running: 'text-amber-500',
};

const scope = computed(() => {
  const parts = [props.job.sourceId, props.job.table].filter((p) => p != null && p !== '');
  return parts.join(' · ');
});

function formatTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

/** "4 min", "12 s": how long a job took, or has been running. */
function duration(job: Job): string {
  const end = job.finishedAt ?? Math.floor(Date.now() / 1000);
  const seconds = Math.max(0, end - job.startedAt);
  if (seconds < 90) {
    return `${seconds} s`;
  }
  return `${Math.round(seconds / 60)} min`;
}

async function cancel(): Promise<void> {
  cancelling.value = true;
  try {
    if (props.job.kind === 'agent_run') {
      await agentApi.cancel(Number(props.job.id));
    } else if (props.job.kind === 'enrichment_run') {
      await enrichFnApi.cancelRun(props.job.id);
    }
    emit('cancelled');
  } finally {
    cancelling.value = false;
  }
}
</script>

<template>
  <div class="flex items-start gap-3 px-4 py-3">
    <UIcon
      v-if="job.status === 'running'"
      name="i-lucide-loader-circle"
      class="mt-0.5 h-4 w-4 flex-shrink-0 animate-spin text-amber-500"
    />
    <UIcon v-else :name="KIND_ICONS[job.kind]" class="mt-0.5 h-4 w-4 flex-shrink-0 text-muted" />
    <div class="min-w-0 flex-1">
      <p class="text-sm text-highlighted">
        {{ job.label }}
        <span v-if="scope" class="text-muted"> · {{ scope }}</span>
      </p>
      <p class="text-xs text-muted">
        <span :class="statusColor[job.status] ?? 'text-muted'">{{ job.status }}</span>
        · {{ formatTime(job.startedAt) }} · {{ duration(job) }}
        <template v-if="job.detail"> · {{ job.detail }}</template>
      </p>
    </div>
    <UButton
      v-if="job.status === 'running' && job.cancellable"
      size="xs"
      color="neutral"
      variant="ghost"
      :loading="cancelling"
      @click="() => void cancel()"
    >
      Cancel
    </UButton>
  </div>
</template>
