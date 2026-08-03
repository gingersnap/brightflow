<script setup lang="ts">
/**
 * Review queue for intent labelling. Defaults to agent-proposed, unconfirmed
 * rows because seed-label quality caps classifier accuracy — that filter is
 * where curation time buys the most. label_document actions are whole-set
 * replaces, so Confirm just re-dispatches the agent's set as a human label.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';

import { useCuration } from '@/composables/useCuration';
import { taxonomyApi } from '@/services/api';
import type { CurationDoc } from '@/types/generated';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const curation = useCuration();
const queryCache = useQueryCache();

/**
 * Filter tabs. "agent" is the review queue proper: rows the LLM proposed labels
 * for that no human has confirmed. Seed quality caps everything downstream, so
 * this is where curation time actually buys accuracy.
 */
const FILTERS = [
  { value: 'agent', label: 'Needs review' },
  { value: 'human', label: 'Curated' },
  { value: 'unlabelled', label: 'Unlabelled' },
  { value: 'all', label: 'All' },
] as const;

const filter = ref<string>('agent');
const busyRow = ref<string | null>(null);

const { data: queue, isLoading } = useQuery({
  key: () => ['taxonomy-queue', props.sourceId, props.table, filter.value],
  query: () => taxonomyApi.queue(props.sourceId, props.table, { filter: filter.value, limit: 50 }),
});

const categories = computed(() => queue.value?.categories ?? []);
const docs = computed(() => queue.value?.docs ?? []);

async function refresh(): Promise<void> {
  await Promise.all([
    queryCache.invalidateQueries({ key: ['taxonomy-queue', props.sourceId, props.table] }),
    queryCache.invalidateQueries({ key: ['taxonomy', props.sourceId, props.table] }),
  ]);
}

/** Replace a row's whole label set — the action is a replace, so send the full set. */
async function setLabels(doc: CurationDoc, next: string[]): Promise<void> {
  busyRow.value = doc.rowId;
  try {
    await curation.dispatch({
      kind: 'label_document',
      source_id: props.sourceId,
      table: props.table,
      row_id: doc.rowId,
      categories: next,
    });
    await refresh();
  } finally {
    busyRow.value = null;
  }
}

function toggle(doc: CurationDoc, name: string): void {
  const next = doc.categories.includes(name)
    ? doc.categories.filter((c) => c !== name)
    : [...doc.categories, name];
  void setLabels(doc, next);
}

/** Confirm the agent's proposal as-is: re-dispatch the same set as a human. */
function confirm(doc: CurationDoc): void {
  void setLabels(doc, doc.categories);
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex items-start justify-between gap-3">
      <div class="flex flex-col gap-1">
        <h3 class="text-sm font-medium text-highlighted">Review queue</h3>
        <p class="text-sm text-muted">
          Correct what each ticket is labelled. The classifier can only be as good as these labels —
          this is the real cost of the feature, and the highest-leverage time you can spend on it.
        </p>
      </div>
      <UBadge v-if="queue" size="lg" color="neutral" variant="subtle">
        {{ queue.pendingReview }} pending
      </UBadge>
    </div>

    <div class="flex gap-1">
      <UButton
        v-for="f in FILTERS"
        :key="f.value"
        size="md"
        :color="filter === f.value ? 'primary' : 'neutral'"
        :variant="filter === f.value ? 'solid' : 'ghost'"
        @click="filter = f.value"
      >
        {{ f.label }}
      </UButton>
    </div>

    <div v-if="isLoading" class="flex items-center gap-2 text-sm text-muted">
      <UIcon name="i-lucide-loader-circle" class="animate-spin" />
      Loading tickets…
    </div>

    <div v-else-if="categories.length === 0" class="rounded-lg border border-default p-4">
      <p class="text-sm text-muted">Define the taxonomy first — there is nothing to label with.</p>
    </div>

    <div v-else-if="docs.length === 0" class="rounded-lg border border-default p-4">
      <p class="text-sm text-muted">Nothing here.</p>
    </div>

    <ul v-else class="flex flex-col gap-3">
      <li
        v-for="doc in docs"
        :key="doc.rowId"
        class="flex flex-col gap-3 rounded-lg border border-default bg-elevated p-4"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="flex min-w-0 flex-col gap-1">
            <span class="text-sm font-medium text-highlighted">
              {{ doc.title ?? '(untitled)' }}
            </span>
            <p v-if="doc.body" class="line-clamp-3 text-sm text-muted">{{ doc.body }}</p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <!-- Format cluster shown deliberately: it is what the ticket LOOKS
                 like, next to what it is ABOUT. Seeing them disagree is the
                 point of this whole feature. -->
            <UBadge v-if="doc.clusterId != null" size="lg" color="neutral" variant="outline">
              format #{{ doc.clusterId }}
            </UBadge>
            <UButton
              v-if="doc.source === 'agent'"
              size="md"
              color="primary"
              variant="soft"
              icon="i-lucide-check"
              :loading="busyRow === doc.rowId"
              @click="confirm(doc)"
            >
              Confirm
            </UButton>
          </div>
        </div>

        <div class="flex flex-wrap gap-1.5">
          <UButton
            v-for="category in categories"
            :key="category.id"
            size="md"
            :color="doc.categories.includes(category.name) ? 'primary' : 'neutral'"
            :variant="doc.categories.includes(category.name) ? 'solid' : 'outline'"
            :disabled="busyRow === doc.rowId"
            @click="toggle(doc, category.name)"
          >
            {{ category.name }}
          </UButton>
        </div>
      </li>
    </ul>
  </div>
</template>
