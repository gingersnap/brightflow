<script setup lang="ts">
/**
 * The log: every action — human or agent — and every background job once
 * it finished, in one order. A person's actions are rows; an agent run's
 * actions collapse into one card with the run's note (`RunCard`); a
 * finished sync, enrichment or insight run is a job row placed by the time
 * it finished, so it sits next to what it caused. Per-entry
 * approve/reject/undo live on the row and appear only when there is
 * something to decide. The bulk "Accept all" is the one guarded path; see
 * approveAll for what it confirms and deliberately does not promise.
 *
 * The runs behind the cards and the finished jobs come from their lists,
 * refetched on every pushed `agentRun` or `insightsComputed` frame; a run
 * older than the list's window still groups, without its header details.
 */

import { useQuery } from '@pinia/colada';
import { computed, onBeforeUnmount, ref } from 'vue';

import ActionRow from '@/components/actions/ActionRow.vue';
import JobRow from '@/components/actions/JobRow.vue';
import RunCard from '@/components/actions/RunCard.vue';
import { useCuration } from '@/composables/useCuration';
import { agentApi, jobsApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';
import type { AgentRunResponse } from '@/types/generated';

import { type FeedItem, groupFeed, mergeJobs } from './activityGroups';

/** Runs fetched for the cards; the feed holds 100 entries, so this covers it. */
const RUNS_WINDOW = 200;

// Queries fire on setup; the store merges WS pushes on top.
const curation = useCuration();
const connection = useConnectionStore();

const entries = computed(() => curation.store.feed);
const pending = computed(() => curation.store.pendingCount);

const runsTick = ref(0);
const { data: runList } = useQuery({
  key: () => ['agent-runs-for-feed', runsTick.value, curation.store.resyncTick],
  query: async () => (await agentApi.list(RUNS_WINDOW)) ?? [],
});
const { data: jobList } = useQuery({
  key: () => ['jobs-for-log', runsTick.value, curation.store.resyncTick],
  query: async () => (await jobsApi.list(100)) ?? [],
});
const stopRunEvents = connection.onMessage('agentRun', () => {
  runsTick.value += 1;
});
const stopInsightEvents = connection.onMessage('insightsComputed', () => {
  runsTick.value += 1;
});
onBeforeUnmount(() => {
  stopRunEvents();
  stopInsightEvents();
});

const runsById = computed(() => {
  const map = new Map<number, AgentRunResponse>();
  for (const run of runList.value ?? []) {
    map.set(run.id, run);
  }
  return map;
});

const items = computed(() =>
  mergeJobs(groupFeed(entries.value, runsById.value), jobList.value ?? []),
);

function itemKey(item: FeedItem): string {
  switch (item.kind) {
    case 'run': {
      return `run-${item.runId}`;
    }
    case 'job': {
      return `job-${item.job.kind}-${item.job.id}`;
    }
    case 'entry': {
      return `entry-${item.entry.id}`;
    }
  }
}

/**
 * Bulk approve. Confirms first, because this applies every pending proposal at
 * once and there is no matching "undo all" — reversing it means the same
 * one-by-one grind this button exists to avoid.
 *
 * The count comes from the server, not from the visible feed (capped at 100), so
 * the dialog promises exactly what happens. The wording deliberately does NOT
 * claim the batch is undoable: some action kinds aren't, and even
 * `define_taxonomy_category` refuses to undo once rows carry its label. The feed
 * shows an undo control per action where one genuinely exists.
 */
async function approveAll(): Promise<void> {
  const n = pending.value;
  if (n === 0) {
    return;
  }
  const confirmed = window.confirm(
    `Approve all ${n} pending proposal${n === 1 ? '' : 's'}?\n\n` +
      `They are applied oldest first, in the order they were proposed. ` +
      `There is no bulk undo — reversing this means undoing them individually.`,
  );
  if (!confirmed) {
    return;
  }
  const result = await curation.approveAll();
  if (result != null && result.failed > 0) {
    window.alert(
      `Applied ${result.approved} of ${result.total}. ${result.failed} failed — ` +
        `see the feed for details.`,
    );
  }
}
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex items-start justify-between gap-3 border-b border-default px-4 py-3">
      <div class="min-w-0">
        <h2 class="text-sm font-semibold text-highlighted">Logs</h2>
        <p class="text-sm text-muted">
          Every action by you or an agent, and every job that finished.
        </p>
      </div>
      <UButton
        v-if="pending > 0"
        size="md"
        color="primary"
        variant="soft"
        icon="i-lucide-check-check"
        class="flex-shrink-0"
        :loading="curation.approvingAll.value"
        @click="() => void approveAll()"
      >
        Accept all ({{ pending }})
      </UButton>
    </div>

    <p
      v-if="curation.store.lastError"
      class="border-b border-default px-4 py-2 text-sm text-red-500"
    >
      {{ curation.store.lastError }}
    </p>
    <div class="flex-1 overflow-y-auto">
      <p v-if="items.length === 0" class="p-4 text-sm text-muted">Nothing has happened yet.</p>
      <ul v-else class="divide-y divide-default">
        <li v-for="item in items" :key="itemKey(item)">
          <RunCard
            v-if="item.kind === 'run'"
            :run-id="item.runId"
            :run="item.run"
            :entries="item.entries"
          />
          <JobRow v-else-if="item.kind === 'job'" :job="item.job" />
          <ActionRow v-else :entry="item.entry" />
        </li>
      </ul>
    </div>
  </div>
</template>
