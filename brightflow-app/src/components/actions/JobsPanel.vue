<script setup lang="ts">
/**
 * Running jobs: every background run — connector syncs, enrichment runs,
 * agent runs, insight runs — in one list, what is running first and then
 * what recently finished. Every kind pushes over the socket (agent and
 * insight runs on their own frames, syncs and enrichment runs on `job`
 * frames), so the list refetches on each push and never polls.
 */

import { useQuery } from '@pinia/colada';
import { computed, onBeforeUnmount, ref } from 'vue';

import JobRow from '@/components/actions/JobRow.vue';
import { jobsApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';

const connection = useConnectionStore();
const tick = ref(0);

const { data, refetch } = useQuery({
  key: () => ['jobs', tick.value],
  query: async () => (await jobsApi.list(100)) ?? [],
});

const jobs = computed(() => data.value ?? []);
const running = computed(() => jobs.value.filter((j) => j.status === 'running'));
const finished = computed(() => jobs.value.filter((j) => j.status !== 'running'));

const bump = (): void => {
  tick.value += 1;
};
const stops = [
  connection.onMessage('agentRun', bump),
  connection.onMessage('insightsComputed', bump),
  connection.onMessage('job', bump),
];
onBeforeUnmount(() => {
  for (const stop of stops) {
    stop();
  }
});
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
