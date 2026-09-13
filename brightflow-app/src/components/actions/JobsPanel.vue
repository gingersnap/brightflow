<script setup lang="ts">
/**
 * Running jobs: every background run — connector syncs, enrichment runs,
 * agent runs, insight runs — in one list, what is running first and then
 * what recently finished. Agent and insight runs push over the socket;
 * syncs and enrichment runs do not yet, so while anything is running the
 * list refreshes every few seconds, and stops when nothing is.
 */

import { useQuery } from '@pinia/colada';
import { useIntervalFn } from '@vueuse/core';
import { computed, onBeforeUnmount, ref, watch } from 'vue';

import JobRow from '@/components/actions/JobRow.vue';
import { jobsApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';

/** Refresh cadence while a job is running; the same cadence the tools use. */
const LIVE_REFRESH_MS = 3000;

const connection = useConnectionStore();
const tick = ref(0);

const { data, refetch } = useQuery({
  key: () => ['jobs', tick.value],
  query: async () => (await jobsApi.list(100)) ?? [],
});

const jobs = computed(() => data.value ?? []);
const running = computed(() => jobs.value.filter((j) => j.status === 'running'));
const finished = computed(() => jobs.value.filter((j) => j.status !== 'running'));

const stopAgentEvents = connection.onMessage('agentRun', () => {
  tick.value += 1;
});
const stopInsightEvents = connection.onMessage('insightsComputed', () => {
  tick.value += 1;
});
onBeforeUnmount(() => {
  stopAgentEvents();
  stopInsightEvents();
});

const { pause, resume } = useIntervalFn(() => void refetch(), LIVE_REFRESH_MS, {
  immediate: false,
});
watch(
  () => running.value.length > 0,
  (live) => {
    if (live) {
      resume();
    } else {
      pause();
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="border-b border-default px-4 py-3">
      <h2 class="text-sm font-semibold text-highlighted">Running jobs</h2>
      <p class="text-sm text-muted">
        Syncs, enrichment, agent runs and insight runs — what is running now, then what finished.
      </p>
    </div>
    <div class="flex-1 overflow-y-auto">
      <section>
        <h3
          class="border-b border-default bg-muted/10 px-4 py-1.5 text-xs font-semibold tracking-wider text-muted uppercase"
        >
          Running ({{ running.length }})
        </h3>
        <p v-if="running.length === 0" class="px-4 py-3 text-sm text-muted">Nothing is running.</p>
        <div v-else class="divide-y divide-default">
          <JobRow
            v-for="job in running"
            :key="`${job.kind}:${job.id}`"
            :job="job"
            @cancelled="() => void refetch()"
          />
        </div>
      </section>
      <section>
        <h3
          class="border-y border-default bg-muted/10 px-4 py-1.5 text-xs font-semibold tracking-wider text-muted uppercase"
        >
          Finished
        </h3>
        <p v-if="finished.length === 0" class="px-4 py-3 text-sm text-muted">No jobs yet.</p>
        <div v-else class="divide-y divide-default">
          <JobRow v-for="job in finished" :key="`${job.kind}:${job.id}`" :job="job" />
        </div>
      </section>
    </div>
  </div>
</template>
