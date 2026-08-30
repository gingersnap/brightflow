<script setup lang="ts">
/**
 * The table's vocabularies as a two-level tree per kind — categories with
 * their subcategories, feedback categories, products (area → component),
 * competitors — with the cap meter, freeze toggle, rename / redefine /
 * delete, and the audit line naming who did what. Every write dispatches
 * through the action bus; LLM induction runs start from here so a proposed
 * list lands as a reviewable diff against the current one.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';

import { useCuration } from '@/composables/useCuration';
import { agentApi, taxonomyApi } from '@/services/api';
import { useCurationStore } from '@/stores/curation';
import type { TaxonomyCategory } from '@/types/generated';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

type VocabKind = 'category' | 'subcategory' | 'feedback_category' | 'product' | 'competitor';

/** Kinds with a root level, in display order, with their caps. */
const KINDS: { kind: VocabKind; label: string; cap: number; childKind: VocabKind | null }[] = [
  { kind: 'category', label: 'Categories', cap: 10, childKind: 'subcategory' },
  { kind: 'feedback_category', label: 'Feedback categories', cap: 10, childKind: null },
  { kind: 'product', label: 'Products', cap: 20, childKind: 'product' },
  { kind: 'competitor', label: 'Competitors', cap: 20, childKind: null },
];

const curation = useCuration();
const curationStore = useCurationStore();
const queryCache = useQueryCache();

const { data: taxonomy, isLoading } = useQuery({
  key: () => ['taxonomy', props.sourceId, props.table],
  query: () => taxonomyApi.overview(props.sourceId, props.table),
});

async function refresh(): Promise<void> {
  await queryCache.invalidateQueries({ key: ['taxonomy', props.sourceId, props.table] });
}

const entries = computed(() => taxonomy.value?.categories ?? []);

function roots(kind: VocabKind): TaxonomyCategory[] {
  return entries.value.filter((e) => e.kind === kind && e.parentId === 0);
}

function children(parent: TaxonomyCategory, childKind: VocabKind): TaxonomyCategory[] {
  return entries.value.filter((e) => e.kind === childKind && e.parentId === parent.id);
}

/** Last logged action touching an entry: "renamed by jens", "proposed by run 12". */
function auditLine(entry: TaxonomyCategory): string | null {
  const hit = curationStore.feed.find((a) => {
    const params = a.params as Record<string, unknown> | null;
    const result = a.result as Record<string, unknown> | null;
    return (
      a.actionKind.endsWith('_taxonomy_category') &&
      (params?.['category_id'] === entry.id || result?.['categoryId'] === entry.id)
    );
  });
  if (hit == null) {
    return null;
  }
  const verb = hit.actionKind.replace('_taxonomy_category', '').replace('define', 'defined');
  const who =
    hit.actorType === 'agent' ? `run #${hit.agentRunId ?? '?'}` : (hit.userId ?? 'someone');
  return `${verb === 'defined' ? verb : `${verb}d`} by ${who}`;
}

const busy = ref(false);
const lastRunNote = ref<string | null>(null);

async function dispatch(action: Parameters<typeof curation.dispatch>[0]): Promise<void> {
  busy.value = true;
  try {
    await curation.dispatch(action);
    await refresh();
  } finally {
    busy.value = false;
  }
}

const scope = () => ({ source_id: props.sourceId, table: props.table });

async function define(kind: VocabKind, parent: TaxonomyCategory | null): Promise<void> {
  const name = window.prompt(
    parent == null ? `New ${kind.replace('_', ' ')}` : `New entry under "${parent.name}"`,
  );
  if (name == null || name.trim() === '') {
    return;
  }
  const description = window.prompt('One-sentence definition (what the model classifies against)');
  await dispatch({
    kind: 'define_taxonomy_category',
    ...scope(),
    name: name.trim(),
    description: description != null && description.trim() !== '' ? description.trim() : null,
    vocab_kind: kind,
    parent_id: parent?.id ?? 0,
  });
}

async function rename(entry: TaxonomyCategory): Promise<void> {
  const name = window.prompt('Rename (label only — nothing recomputes)', entry.name);
  if (name == null || name.trim() === '' || name.trim() === entry.name) {
    return;
  }
  await dispatch({
    kind: 'rename_taxonomy_category',
    ...scope(),
    category_id: entry.id,
    name: name.trim(),
  });
}

async function redefine(entry: TaxonomyCategory): Promise<void> {
  const description = window.prompt(
    'Redefine (the definition changes — every affected ticket recomputes on the next run)',
    entry.description ?? '',
  );
  if (description == null || description.trim() === (entry.description ?? '')) {
    return;
  }
  await dispatch({
    kind: 'redefine_taxonomy_category',
    ...scope(),
    category_id: entry.id,
    description: description.trim() === '' ? null : description.trim(),
  });
}

async function toggleFrozen(entry: TaxonomyCategory): Promise<void> {
  await dispatch({
    kind: 'freeze_taxonomy_category',
    ...scope(),
    category_id: entry.id,
    frozen: !entry.frozen,
  });
}

async function remove(entry: TaxonomyCategory): Promise<void> {
  if (!window.confirm(`Delete "${entry.name}"? Undoable from the activity feed.`)) {
    return;
  }
  await dispatch({ kind: 'delete_taxonomy_category', ...scope(), category_id: entry.id });
}

async function induce(kind: string, parent?: TaxonomyCategory): Promise<void> {
  busy.value = true;
  lastRunNote.value = null;
  try {
    const run = await agentApi.start({
      kind,
      sourceId: props.sourceId,
      table: props.table,
      mode: 'propose',
      ...(parent == null ? {} : { parentId: parent.id }),
    });
    lastRunNote.value =
      run == null
        ? 'Could not start the induction run — see the activity feed'
        : `Run #${run.id} started; proposals arrive in the activity feed as a diff to approve`;
  } finally {
    busy.value = false;
  }
}

function inductionKind(kind: VocabKind): string | null {
  switch (kind) {
    case 'category': {
      return 'propose_categories';
    }
    case 'feedback_category': {
      return 'propose_feedback_categories';
    }
    default: {
      return null;
    }
  }
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <div class="flex flex-col gap-1">
      <h3 class="text-sm font-medium text-highlighted">Vocabularies</h3>
      <p class="text-sm text-muted">
        Every closed list the enrichment resolves against. Induced lists are proposed from ticket
        summaries and reviewed as a diff; imported lists come from your catalog. A rename is free; a
        redefinition recomputes.
      </p>
      <p v-if="lastRunNote" class="text-sm text-default">{{ lastRunNote }}</p>
    </div>

    <div v-if="isLoading" class="flex items-center gap-2 text-sm text-muted">
      <UIcon name="i-lucide-loader-circle" class="animate-spin" />
      Loading vocabularies…
    </div>

    <section v-for="level in KINDS" v-else :key="level.kind" class="flex flex-col gap-2">
      <div class="flex items-center justify-between gap-3">
        <div class="flex items-center gap-2">
          <h4 class="text-sm font-medium text-highlighted">{{ level.label }}</h4>
          <UBadge
            size="lg"
            variant="subtle"
            :color="roots(level.kind).length >= level.cap ? 'warning' : 'neutral'"
          >
            {{ roots(level.kind).length }} / {{ level.cap }}
          </UBadge>
        </div>
        <div class="flex gap-1">
          <UButton
            v-if="inductionKind(level.kind)"
            size="md"
            color="neutral"
            variant="outline"
            icon="i-lucide-sparkles"
            :loading="busy"
            @click="() => void induce(inductionKind(level.kind) ?? '')"
          >
            Propose
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="outline"
            icon="i-lucide-plus"
            :disabled="busy"
            @click="() => void define(level.kind, null)"
          >
            Add
          </UButton>
        </div>
      </div>

      <p v-if="roots(level.kind).length === 0" class="text-sm text-muted">
        Nothing yet<template v-if="inductionKind(level.kind)">
          — propose a list from summaries, or add by hand</template
        >.
      </p>

      <ul v-else class="flex flex-col gap-2">
        <li
          v-for="entry in roots(level.kind)"
          :key="entry.id"
          class="rounded-lg border border-default bg-elevated p-3"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="flex min-w-0 flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium text-highlighted">{{ entry.name }}</span>
                <UIcon v-if="entry.frozen" name="i-lucide-lock" class="size-4 text-muted" />
              </div>
              <p v-if="entry.description" class="line-clamp-2 text-sm text-muted">
                {{ entry.description }}
              </p>
              <p v-if="auditLine(entry)" class="text-sm text-dimmed">{{ auditLine(entry) }}</p>
            </div>
            <div class="flex shrink-0 gap-1">
              <UButton
                v-if="level.kind === 'category'"
                size="md"
                color="neutral"
                variant="ghost"
                icon="i-lucide-sparkles"
                aria-label="Propose subcategories"
                :disabled="busy"
                @click="() => void induce('propose_subcategories', entry)"
              />
              <UButton
                v-if="level.childKind"
                size="md"
                color="neutral"
                variant="ghost"
                icon="i-lucide-plus"
                aria-label="Add child"
                :disabled="busy"
                @click="() => void define(level.childKind ?? level.kind, entry)"
              />
              <UButton
                size="md"
                color="neutral"
                variant="ghost"
                :icon="entry.frozen ? 'i-lucide-lock-open' : 'i-lucide-lock'"
                :aria-label="entry.frozen ? 'Unfreeze' : 'Freeze'"
                :disabled="busy"
                @click="() => void toggleFrozen(entry)"
              />
              <UButton
                size="md"
                color="neutral"
                variant="ghost"
                icon="i-lucide-pencil"
                aria-label="Rename"
                :disabled="busy || entry.frozen"
                @click="() => void rename(entry)"
              />
              <UButton
                size="md"
                color="neutral"
                variant="ghost"
                icon="i-lucide-file-pen"
                aria-label="Redefine"
                :disabled="busy || entry.frozen"
                @click="() => void redefine(entry)"
              />
              <UButton
                size="md"
                color="neutral"
                variant="ghost"
                icon="i-lucide-trash-2"
                aria-label="Delete"
                :disabled="busy || entry.frozen"
                @click="() => void remove(entry)"
              />
            </div>
          </div>

          <ul
            v-if="level.childKind && children(entry, level.childKind).length > 0"
            class="mt-2 flex flex-col gap-1 border-l border-default pl-3"
          >
            <li
              v-for="child in children(entry, level.childKind)"
              :key="child.id"
              class="flex items-start justify-between gap-3"
            >
              <div class="flex min-w-0 flex-col">
                <span class="text-sm text-highlighted">
                  {{ child.name }}
                  <UIcon v-if="child.frozen" name="i-lucide-lock" class="size-3 text-muted" />
                </span>
                <span v-if="child.description" class="line-clamp-1 text-sm text-muted">
                  {{ child.description }}
                </span>
                <span v-if="auditLine(child)" class="text-sm text-dimmed">{{
                  auditLine(child)
                }}</span>
              </div>
              <div class="flex shrink-0 gap-1">
                <UButton
                  size="md"
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-pencil"
                  aria-label="Rename"
                  :disabled="busy || child.frozen"
                  @click="() => void rename(child)"
                />
                <UButton
                  size="md"
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-file-pen"
                  aria-label="Redefine"
                  :disabled="busy || child.frozen"
                  @click="() => void redefine(child)"
                />
                <UButton
                  size="md"
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-trash-2"
                  aria-label="Delete"
                  :disabled="busy || child.frozen"
                  @click="() => void remove(child)"
                />
              </div>
            </li>
          </ul>
        </li>
      </ul>
    </section>
  </div>
</template>
