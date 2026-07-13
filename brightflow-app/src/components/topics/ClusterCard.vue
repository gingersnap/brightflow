<script setup lang="ts">
import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { topicsApi } from '@/services/api';
import { useCurationStore } from '@/stores/curation';
import type { ClusterSummary, DocRef } from '@/types/generated';

import { clusterColor } from './colors';

const props = defineProps<{
  cluster: ClusterSummary;
  index: number;
  sourceId: string;
  table: string;
}>();

const emit = defineEmits<{
  openDoc: [doc: DocRef];
}>();

const expanded = ref(false);

// Curated clusters (renamed or labeled) headline with their name.
// Uncurated clusters show c-TF-IDF terms, which read better than the auto-generated name.
const headline = computed(() =>
  props.cluster.curated
    ? props.cluster.name
    : props.cluster.topTerms.join(', ') || props.cluster.name,
);
const subline = computed(() => (props.cluster.curated ? props.cluster.topTerms.join(', ') : ''));
const color = computed(() => clusterColor(props.index));

const { data: detail, isLoading } = useQuery({
  key: () => ['topics', props.sourceId, props.table, 'cluster', props.cluster.id],
  query: () => topicsApi.clusterDetail(props.sourceId, props.table, props.cluster.id),
  enabled: () => expanded.value,
});

// ── Curation actions (same dispatch path an LLM agent uses) ──────────────
const curation = useCurationStore();
const queryCache = useQueryCache();

async function afterAction(): Promise<void> {
  await queryCache.invalidateQueries({ key: ['topics', props.sourceId, props.table] });
}

async function renameCluster(): Promise<void> {
  const name = window.prompt('New cluster name', props.cluster.name);
  if (name == null || name.trim() === '') {
    return;
  }
  await curation.dispatch({
    kind: 'rename_cluster',
    source_id: props.sourceId,
    table: props.table,
    cluster_id: props.cluster.id,
    name: name.trim(),
  });
  await afterAction();
}

async function mergeInto(): Promise<void> {
  const target = window.prompt('Merge into cluster id');
  const targetId = target == null || target.trim() === '' ? Number.NaN : Math.trunc(Number(target));
  if (Number.isNaN(targetId)) {
    return;
  }
  await curation.dispatch({
    kind: 'merge_clusters',
    source_id: props.sourceId,
    table: props.table,
    from_cluster_id: props.cluster.id,
    into_cluster_id: targetId,
  });
  await afterAction();
}

async function markNoise(): Promise<void> {
  await curation.dispatch({
    kind: 'mark_cluster_noise',
    source_id: props.sourceId,
    table: props.table,
    cluster_id: props.cluster.id,
    is_noise: true,
  });
  await afterAction();
}

async function assignLabel(): Promise<void> {
  const label = window.prompt('Label for this cluster');
  if (label == null || label.trim() === '') {
    return;
  }
  await curation.dispatch({
    kind: 'assign_cluster_label',
    source_id: props.sourceId,
    table: props.table,
    cluster_id: props.cluster.id,
    label: label.trim(),
  });
  await afterAction();
}

async function excludeTerm(): Promise<void> {
  const term = window.prompt('Term to exclude from naming');
  if (term == null || term.trim() === '') {
    return;
  }
  await curation.dispatch({
    kind: 'exclude_term',
    source_id: props.sourceId,
    table: props.table,
    term: term.trim(),
  });
  await afterAction();
}

const menuItems = [
  [
    { label: 'Rename…', icon: 'i-lucide-pencil', onSelect: () => void renameCluster() },
    { label: 'Assign label…', icon: 'i-lucide-tag', onSelect: () => void assignLabel() },
    { label: 'Merge into…', icon: 'i-lucide-combine', onSelect: () => void mergeInto() },
    { label: 'Exclude term…', icon: 'i-lucide-filter-x', onSelect: () => void excludeTerm() },
  ],
  [{ label: 'Mark as noise', icon: 'i-lucide-eye-off', onSelect: () => void markNoise() }],
];
</script>

<template>
  <div class="overflow-hidden rounded-lg border border-default bg-elevated">
    <CollapsibleSection v-model:open="expanded">
      <template #title>
        <span
          class="inline-block h-3 w-3 shrink-0 rounded-full"
          :style="{ backgroundColor: color }"
        />
        <div class="flex flex-1 flex-col gap-1">
          <div class="flex items-baseline justify-between gap-3">
            <span class="line-clamp-2 text-sm font-medium text-highlighted">
              {{ headline }}
            </span>
            <span class="shrink-0 text-sm text-muted">
              {{ cluster.size.toLocaleString() }}
            </span>
          </div>
          <p v-if="subline" class="line-clamp-1 text-sm text-muted">{{ subline }}</p>
        </div>
      </template>

      <template #actions>
        <UDropdownMenu :items="menuItems">
          <UButton
            size="xs"
            color="neutral"
            variant="ghost"
            icon="i-lucide-more-horizontal"
            aria-label="Cluster actions"
          />
        </UDropdownMenu>
      </template>

      <div class="border-t border-default px-4 py-3">
        <div v-if="isLoading" class="text-sm text-muted">Loading…</div>
        <div v-else-if="detail">
          <ul class="flex flex-col gap-1">
            <li v-for="sample in detail.samples" :key="sample.id">
              <button
                type="button"
                class="line-clamp-2 w-full rounded px-2 py-1.5 text-left text-sm text-default transition-colors hover:bg-accented/60"
                @click="emit('openDoc', sample)"
              >
                <span v-if="sample.number != null" class="text-muted">#{{ sample.number }}</span>
                {{ sample.title ?? '(untitled)' }}
              </button>
            </li>
          </ul>
        </div>
      </div>
    </CollapsibleSection>
  </div>
</template>
