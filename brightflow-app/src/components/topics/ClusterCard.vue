<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { computed, ref } from 'vue';

import { topicsApi } from '@/services/api';
import type { ClusterSummary, IssueRef } from '@/types/generated';

import { clusterColor } from './colors';

const props = defineProps<{
  cluster: ClusterSummary;
  index: number;
  sourceId: string;
  table: string;
}>();

const emit = defineEmits<{
  openIssue: [issue: IssueRef];
}>();

const expanded = ref(false);

const headlineTerms = computed(() => props.cluster.topTerms.join(', ') || props.cluster.name);
const color = computed(() => clusterColor(props.index));

const { data: detail, isLoading } = useQuery({
  key: () => ['topics', props.sourceId, props.table, 'cluster', props.cluster.id],
  query: () => topicsApi.clusterDetail(props.sourceId, props.table, props.cluster.id),
  enabled: () => expanded.value,
});

function toggle(): void {
  expanded.value = !expanded.value;
}
</script>

<template>
  <div class="overflow-hidden rounded-lg border border-default bg-elevated">
    <button
      type="button"
      class="flex w-full items-start gap-3 p-4 text-left transition-colors hover:bg-accented/40"
      @click="toggle"
    >
      <span
        class="mt-1.5 inline-block h-3 w-3 shrink-0 rounded-full"
        :style="{ backgroundColor: color }"
      />
      <div class="flex flex-1 flex-col gap-2">
        <div class="flex items-baseline justify-between gap-3">
          <span class="line-clamp-2 text-sm font-medium text-highlighted">
            {{ headlineTerms }}
          </span>
          <span class="shrink-0 text-sm text-muted">
            {{ cluster.size.toLocaleString() }}
          </span>
        </div>
      </div>
      <UIcon
        :name="expanded ? 'i-lucide-chevron-down' : 'i-lucide-chevron-right'"
        class="mt-1 size-4 shrink-0 text-muted"
      />
    </button>

    <div v-if="expanded" class="border-t border-default px-4 py-3">
      <div v-if="isLoading" class="text-sm text-muted">Loading…</div>
      <div v-else-if="detail">
        <ul class="flex flex-col gap-1">
          <li v-for="sample in detail.samples" :key="sample.id">
            <button
              type="button"
              class="line-clamp-2 w-full rounded px-2 py-1.5 text-left text-sm text-default transition-colors hover:bg-accented/60"
              @click="emit('openIssue', sample)"
            >
              <span v-if="sample.number != null" class="text-muted">#{{ sample.number }}</span>
              {{ sample.title ?? '(untitled)' }}
            </button>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
