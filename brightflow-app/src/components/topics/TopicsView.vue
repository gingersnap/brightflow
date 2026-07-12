<script setup lang="ts">
import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';

import ActivityFeed from '@/components/actions/ActivityFeed.vue';
import AgentActions from '@/components/actions/AgentActions.vue';
import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { topicsApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';
import type { SourceTable } from '@/types';
import type { DocRef } from '@/types/generated';

import ClusterCard from './ClusterCard.vue';
import DocDrawer from './DocDrawer.vue';
import TopicsHeader from './TopicsHeader.vue';
import TopicsPie from './TopicsPie.vue';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const sourceStore = useSourceStore();

const enrichableTables = computed(
  () => sourceStore.getSourceById(props.sourceId)?.tables.filter((t) => t.enrichable) ?? [],
);
const activeTable = computed(() => props.table);

function handleSelectTable(table: SourceTable): void {
  router.push({
    name: 'topics-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({
    name: 'topics-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

const queryCache = useQueryCache();
const queryKey = computed(() => ['topics', props.sourceId, activeTable.value]);

const {
  data: overview,
  isLoading,
  error,
} = useQuery({
  key: () => queryKey.value,
  query: () => topicsApi.overview(props.sourceId, activeTable.value ?? ''),
  enabled: () => activeTable.value != null,
});

const reclusterMutation = useMutation({
  mutation: (k?: number) =>
    topicsApi.recluster(props.sourceId, activeTable.value ?? '', k == null ? {} : { k }),
  onSuccess: () => {
    queryCache.invalidateQueries({ key: queryKey.value });
  },
});

const k = ref<number | undefined>(undefined);

function triggerRecluster(): void {
  if (activeTable.value == null) {
    return;
  }
  reclusterMutation.mutate(k.value);
}

const drawerOpen = ref(false);
const drawerDoc = ref<DocRef | null>(null);
const activityOpen = ref(false);

function openDoc(doc: DocRef): void {
  drawerDoc.value = doc;
  drawerOpen.value = true;
}
</script>

<template>
  <div class="flex h-full flex-col">
    <TableSectionPane
      enrichable-only
      :source-id="sourceId"
      :selected-table="table"
      @select-table="handleSelectTable"
      @auto-select-table="handleAutoSelectTable"
    />

    <div v-if="enrichableTables.length === 0" class="p-6">
      <p class="text-sm text-muted">
        Topics needs a table with text content (issues, posts). Sync this source to populate one.
      </p>
    </div>

    <div v-else-if="activeTable == null" class="p-6">
      <p class="text-sm text-muted">Choose a table above to explore topics.</p>
    </div>

    <template v-else>
      <TopicsHeader
        :overview="overview"
        :is-loading="isLoading"
        :is-reclustering="reclusterMutation.isLoading.value"
        :k-input="k"
        :source-id="sourceId"
        :table="activeTable"
        @update:k-input="k = $event"
        @recluster="triggerRecluster"
      />

      <div class="flex-1 overflow-auto p-4">
        <div v-if="error" class="rounded-lg border border-error bg-elevated p-4 text-sm text-error">
          Failed to load topics: {{ String(error) }}
        </div>

        <div
          v-else-if="!overview?.ready"
          class="rounded-lg border border-default bg-elevated p-8 text-center"
        >
          <p class="text-sm text-muted">{{ overview?.reason ?? 'No clusters yet.' }}</p>
          <p class="mt-2 text-sm text-muted">
            Run <code>topics fit</code> from the CLI or click Recluster above.
          </p>
        </div>

        <div v-else class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(280px,360px)_1fr]">
          <div class="lg:sticky lg:top-4 lg:h-[calc(100vh-12rem)]">
            <TopicsPie :clusters="overview.clusters" />
          </div>
          <div class="flex flex-col gap-3">
            <AgentActions
              :source-id="sourceId"
              :table="activeTable"
              :kinds="[
                { kind: 'auto_label', label: 'Suggest labels', icon: 'i-lucide-tags' },
                { kind: 'propose_merges', label: 'Suggest merges', icon: 'i-lucide-combine' },
              ]"
            />
            <ClusterCard
              v-for="(cluster, idx) in overview.clusters"
              :key="cluster.id"
              :cluster="cluster"
              :index="idx"
              :source-id="sourceId"
              :table="activeTable"
              @open-doc="openDoc"
            />
          </div>
        </div>
      </div>

      <DocDrawer v-model="drawerOpen" :doc="drawerDoc" />

      <UButton
        size="md"
        color="neutral"
        variant="soft"
        icon="i-lucide-history"
        class="fixed right-4 bottom-4 z-10 shadow-lg"
        @click="activityOpen = true"
      >
        Activity
      </UButton>
      <USlideover v-model:open="activityOpen" title="Activity">
        <template #body>
          <ActivityFeed />
        </template>
      </USlideover>
    </template>
  </div>
</template>
