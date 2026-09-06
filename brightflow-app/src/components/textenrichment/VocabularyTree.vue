<script setup lang="ts">
/**
 * The table's vocabularies as a two-level tree, one level per requested
 * kind — categories with their subcategories, feedback categories, products
 * (area → component), competitors — with the cap meter, freeze toggle,
 * rename / redefine / delete, and the audit line naming who did what. Row
 * grain is merged in where it exists: each category and subcategory shows
 * the rows classified into it, its share, and a health badge (too small,
 * unused, share out of band); see `vocabularyTree.ts` for the rules. Every
 * write dispatches through the action bus.
 *
 * Induction runs auto-apply: what a run defines is in the tree the moment it
 * finishes, and the result line under the level offers "Undo run" for the
 * whole run. That is the contract — reversibility, not pre-approval. Runs
 * are watched over the pushed `agentRun` frame; "Propose all subcategories"
 * starts one run per eligible parent and counts them down.
 *
 * `other` is never a stored entry, but the tree shows it as a category like
 * any other: sorted by size with the rest, with the share of rows landing
 * there (the number that says whether the level is wrong), and with
 * subcategories of its own — what the vocabulary misses, grouped. Levels
 * with children are collapsed by default and sorted largest first.
 * Induction buttons state their precondition (rows classified at that level)
 * and disable below it instead of starting a run that fails. "Clear all" on
 * the category level is the clean-slate path: one undoable action empties
 * the vocabulary, and the classify function's cache and columns are dropped
 * (not undoable, hence the confirm) while its configuration stays.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { useCuration } from '@/composables/useCuration';
import { agentApi, enrichFnApi, taxonomyApi, ticketsApi, vocabularyApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';
import { useCurationStore } from '@/stores/curation';
import type {
  AgentRunEventPayload,
  TaxonomyCategory,
  ValueCount,
  VocabularyLevelHealth,
} from '@/types/generated';

import {
  buildLevelRows,
  eligibleParents,
  isOtherEntry,
  OTHER_ENTRY,
  sortBySize,
  type EntryRow,
} from './vocabularyTree';

type VocabKind = 'category' | 'subcategory' | 'feedback_category' | 'product' | 'competitor';

const props = defineProps<{
  sourceId: string;
  table: string;
  /** Root levels to render, in this order. */
  kinds: VocabKind[];
}>();

/**
 * Kinds with a root level, with their caps. `induced` levels are proposed
 * from summaries and carry the implicit `other`; imported ones (products,
 * competitors) have neither — an unlisted name becomes an unresolved
 * subject instead.
 */
const KINDS: {
  kind: VocabKind;
  label: string;
  cap: number;
  childKind: VocabKind | null;
  induced: boolean;
}[] = [
  { kind: 'category', label: 'Categories', cap: 10, childKind: 'subcategory', induced: true },
  {
    kind: 'feedback_category',
    label: 'Feedback categories',
    cap: 10,
    childKind: null,
    induced: true,
  },
  { kind: 'product', label: 'Products', cap: 20, childKind: 'product', induced: false },
  { kind: 'competitor', label: 'Competitors', cap: 20, childKind: null, induced: false },
];

const levels = computed(() => KINDS.filter((k) => props.kinds.includes(k.kind)));

/** What the reserved value means, shown where a definition would be. */
const OTHER_DEFINITION =
  'None of the listed entries fit — always offered, never proposed, not counted toward the cap.';

const curation = useCuration();
const curationStore = useCurationStore();
const queryCache = useQueryCache();
const connection = useConnectionStore();

const { data: taxonomy, isLoading } = useQuery({
  key: () => ['taxonomy', props.sourceId, props.table],
  query: () => taxonomyApi.overview(props.sourceId, props.table),
});

// Keys shared with FunctionCard and ClassificationPane: one fetch each.
const { data: health } = useQuery({
  key: () => ['vocabulary-health', props.sourceId, props.table],
  query: () => vocabularyApi.health(props.sourceId, props.table),
});
const { data: tickets } = useQuery({
  key: () => ['tickets-summary', props.sourceId, props.table],
  query: () => ticketsApi.summary(props.sourceId, props.table),
});
const { data: functions } = useQuery({
  key: () => ['enrich-fns', props.sourceId, props.table],
  query: async () => (await enrichFnApi.list(props.sourceId, props.table)) ?? [],
});

/** Counts were computed against an older vocabulary — badges say nothing. */
const stale = computed(() => {
  const classify = (functions.value ?? []).find((f) => f.kind === 'ticket_classify');
  return classify == null || (classify.staleRowCount ?? 0) > 0;
});

async function refresh(): Promise<void> {
  await Promise.all([
    queryCache.invalidateQueries({ key: ['taxonomy', props.sourceId, props.table] }),
    queryCache.invalidateQueries({ key: ['vocabulary-health', props.sourceId, props.table] }),
    queryCache.invalidateQueries({ key: ['tickets-summary', props.sourceId, props.table] }),
    queryCache.invalidateQueries({ key: ['enrich-fns', props.sourceId, props.table] }),
  ]);
}

/* An applied or undone action anywhere (the Activity page included) may have
   touched this table's vocabulary. */
watch(
  () => curationStore.version,
  () => void refresh(),
);

const pct = (share: number): string => `${Math.round(share * 100)}%`;

function levelHealth(kind: VocabKind): VocabularyLevelHealth | null {
  return health.value?.levels.find((l) => l.kind === kind) ?? null;
}

function parentHealth(parent: TaxonomyCategory): VocabularyLevelHealth | null {
  return health.value?.perParent.find((p) => p.parent === parent.name)?.health ?? null;
}

/** Share of rows in `other` at a root level, once the table is classified. */
function rootOtherRate(kind: VocabKind): number | null {
  return levelHealth(kind)?.otherRate ?? null;
}

/** Share of a category's rows whose subcategory is `other`. */
function childOtherRate(parent: TaxonomyCategory): number | null {
  return parentHealth(parent)?.otherRate ?? null;
}

/** Rows classified under a category — what a subcategory proposal reads. */
function rowsUnder(parent: TaxonomyCategory): number {
  return parentHealth(parent)?.rows ?? 0;
}

const inductionMin = computed(() => health.value?.inductionMinRows ?? 50);

/** Why a proposal cannot start yet, or null when it can. */
function subcategoryBlocker(parent: TaxonomyCategory): string | null {
  const rows = rowsUnder(parent);
  return rows >= inductionMin.value
    ? null
    : `Needs ${inductionMin.value} rows classified as "${parent.name}" — has ${rows}`;
}

function categoryBlocker(): string | null {
  const rows = health.value?.classifiedRows ?? 0;
  return rows >= inductionMin.value
    ? null
    : `Needs ${inductionMin.value} classified rows — has ${rows}`;
}

const entries = computed(() => taxonomy.value?.categories ?? []);

function roots(kind: VocabKind): TaxonomyCategory[] {
  return entries.value.filter((e) => e.kind === kind && e.parentId === 0);
}

function children(parent: TaxonomyCategory, childKind: VocabKind): TaxonomyCategory[] {
  return entries.value.filter((e) => e.kind === childKind && e.parentId === parent.id);
}

/** Row grain for a root level: only categories have one. */
function rootCounts(kind: VocabKind): ValueCount[] | null {
  if (kind !== 'category' || tickets.value == null) {
    return null;
  }
  return tickets.value.categories.map((c) => ({ value: c.category, rows: c.rows }));
}

/** Root categories plus the reserved `other`; the parents a fan-out may pick. */
function categoryParents(): TaxonomyCategory[] {
  return [...roots('category'), OTHER_ENTRY];
}

function rootRows(kind: VocabKind): EntryRow[] {
  return sortBySize(
    buildLevelRows({
      entries: kind === 'category' ? categoryParents() : roots(kind),
      counts: rootCounts(kind),
      health: levelHealth(kind),
      stale: stale.value,
    }),
  );
}

function childRows(parent: TaxonomyCategory, childKind: VocabKind): EntryRow[] {
  const counts =
    childKind === 'subcategory'
      ? (tickets.value?.categories.find((c) => c.category === parent.name)?.subcategories ?? null)
      : null;
  return sortBySize(
    buildLevelRows({
      entries: children(parent, childKind),
      counts,
      health: parentHealth(parent),
      stale: stale.value,
    }),
  );
}

/** Expanded root entries by id; everything starts collapsed. */
const expandedIds = ref(new Set<number>());

function isExpanded(entry: TaxonomyCategory): boolean {
  return expandedIds.value.has(entry.id);
}

function toggleExpanded(entry: TaxonomyCategory): void {
  const next = new Set(expandedIds.value);
  if (next.has(entry.id)) {
    next.delete(entry.id);
  } else {
    next.add(entry.id);
  }
  expandedIds.value = next;
}

function badgeColor(kind: 'too-small' | 'unused' | 'out-of-band'): 'warning' | 'neutral' {
  return kind === 'too-small' ? 'warning' : 'neutral';
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
    'Redefine (the definition changes — every affected row recomputes on the next run)',
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

/**
 * The clean slate: every category and subcategory goes in one undoable
 * action, then the classify function forgets its cache and columns (not
 * undoable) but keeps its configuration, so the next run starts from nothing.
 */
async function clearAll(): Promise<void> {
  const classify = (functions.value ?? []).find((f) => f.kind === 'ticket_classify');
  const confirmed = window.confirm(
    'Clear all classification?\n\n' +
      'Every category and subcategory is deleted (undoable from Activity under Settings). ' +
      'The classification columns and cache are dropped and cannot be restored; ' +
      'the function keeps its configuration.',
  );
  if (!confirmed) {
    return;
  }
  busy.value = true;
  try {
    await curation.dispatch({ kind: 'clear_vocabulary', ...scope(), vocab_kind: 'category' });
    if (classify != null) {
      await enrichFnApi.reset(classify.id);
    }
    runLines.value = [];
    fanout.value = null;
    expandedIds.value = new Set();
    await Promise.all([
      refresh(),
      queryCache.invalidateQueries({ key: ['tables-index', props.sourceId] }),
    ]);
  } finally {
    busy.value = false;
  }
}

async function remove(entry: TaxonomyCategory): Promise<void> {
  if (!window.confirm(`Delete "${entry.name}"? Undoable from Activity under Settings.`)) {
    return;
  }
  await dispatch({ kind: 'delete_taxonomy_category', ...scope(), category_id: entry.id });
}

// ---------------------------------------------------------------------------
// Induction runs: auto-apply, watched over WS, undoable per run
// ---------------------------------------------------------------------------

interface RunLine {
  /** Root kind the run defines into — result lines sit under that level. */
  level: VocabKind;
  label: string;
  runId: number | null;
  text: string;
  status: 'running' | 'completed' | 'failed' | 'undone';
}

const runLines = ref<RunLine[]>([]);
/** Fan-out progress for "Propose all": cleared on the next proposal. */
const fanout = ref<{ total: number; done: number } | null>(null);
const undoing = ref<number | null>(null);

function linesFor(kind: VocabKind): RunLine[] {
  return runLines.value.filter((l) => l.level === kind);
}

/** Structural guard for a pushed `agentRun` frame. */
function isAgentRunEvent(
  m: Record<string, unknown>,
): m is Record<string, unknown> & AgentRunEventPayload {
  return typeof m['run'] === 'object' && m['run'] != null;
}

let unsubscribe: (() => void) | null = null;

onMounted(() => {
  unsubscribe = connection.onMessage('agentRun', (message) => {
    if (!isAgentRunEvent(message)) {
      return;
    }
    const { run } = message;
    const line = runLines.value.find((l) => l.runId === run.id);
    if (line == null || run.status === 'running') {
      return;
    }
    line.status = run.status === 'completed' ? 'completed' : 'failed';
    line.text =
      run.status === 'completed'
        ? (run.detail?.split('\n')[0] ?? 'done')
        : `${run.status}: ${run.detail ?? ''}`;
    if (fanout.value != null) {
      fanout.value.done += 1;
    }
    void refresh();
  });
});

onBeforeUnmount(() => {
  unsubscribe?.();
});

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

interface RunRequest {
  kind: string;
  /** Root kind the run defines into — its result line sits under that level. */
  level: VocabKind;
  label: string;
  parent?: TaxonomyCategory;
}

/** Start one run and register its line; a refused start is a failed line. */
async function startRun({ kind, level, label, parent }: RunRequest): Promise<void> {
  const line: RunLine = { level, label, runId: null, text: 'starting…', status: 'running' };
  runLines.value.push(line);
  try {
    const run = await agentApi.start({
      kind,
      sourceId: props.sourceId,
      table: props.table,
      ...(parent == null ? {} : { parentId: parent.id }),
    });
    if (run == null) {
      throw new Error('empty response');
    }
    line.runId = run.id;
    line.text = 'running…';
  } catch (error) {
    line.status = 'failed';
    line.text = error instanceof Error ? error.message : 'could not start';
    if (fanout.value != null) {
      fanout.value.done += 1;
    }
  }
}

async function induce(kind: string, level: VocabKind, parent?: TaxonomyCategory): Promise<void> {
  runLines.value = runLines.value.filter((l) => l.level !== level);
  fanout.value = null;
  busy.value = true;
  try {
    await startRun({
      kind,
      level,
      label: parent == null ? 'Proposal' : `Subcategories of ${parent.name}`,
      ...(parent == null ? {} : { parent }),
    });
  } finally {
    busy.value = false;
  }
}

const parentsReady = computed(() =>
  eligibleParents(categoryParents(), health.value?.perParent ?? [], inductionMin.value),
);

/** One run per eligible parent, in parallel; the backend locks per parent. */
async function induceAllSubcategories(): Promise<void> {
  const parents = parentsReady.value;
  if (parents.length === 0) {
    return;
  }
  runLines.value = runLines.value.filter((l) => l.level !== 'category');
  fanout.value = { total: parents.length, done: 0 };
  busy.value = true;
  try {
    await Promise.all(
      parents.map((p) =>
        startRun({
          kind: 'propose_subcategories',
          level: 'category',
          label: `Subcategories of ${p.name}`,
          parent: p,
        }),
      ),
    );
  } finally {
    busy.value = false;
  }
}

/** Revert everything the run applied, newest first. */
async function undoRun(line: RunLine): Promise<void> {
  if (line.runId == null) {
    return;
  }
  undoing.value = line.runId;
  try {
    const result = await agentApi.undoAll(line.runId);
    if (result == null) {
      line.text = 'undo failed';
      return;
    }
    line.status = 'undone';
    line.text =
      result.failed > 0
        ? `undid ${result.undone} of ${result.total}; ${result.failed} could not be reverted`
        : `undid ${result.undone} action${result.undone === 1 ? '' : 's'}`;
    await refresh();
  } finally {
    undoing.value = null;
  }
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <div class="flex flex-col gap-1">
      <h3 class="text-sm font-medium text-highlighted">Vocabularies</h3>
      <p class="text-sm text-muted">
        Every closed list the enrichment resolves against. Induced lists are proposed from row
        summaries and land directly — up to the cap, fewer when the corpus needs fewer — and a whole
        run can be undone; imported lists come from your catalog. A rename is free; a redefinition
        recomputes.
      </p>
    </div>

    <div v-if="isLoading" class="flex items-center gap-2 text-sm text-muted">
      <UIcon name="i-lucide-loader-circle" class="animate-spin" />
      Loading vocabularies…
    </div>

    <!-- v-for lives inside the v-else, never on it: Vue 3 gives v-if/v-else
         higher priority than v-for, so combining them makes the else-branch a
         keyed fragment where the compiler expects one stable node. With nested
         keyed lists inside, patching corrupts Vue's DOM node tracking and
         throws from getNextHostNode. -->
    <template v-else>
      <section v-for="level in levels" :key="level.kind" class="flex flex-col gap-2">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <h4 class="text-sm font-medium text-highlighted">{{ level.label }}</h4>
            <UBadge
              size="lg"
              variant="subtle"
              :color="roots(level.kind).length >= level.cap ? 'warning' : 'neutral'"
            >
              {{ roots(level.kind).length }} / {{ level.cap }}
            </UBadge>
            <UBadge v-if="fanout && level.kind === 'category'" size="lg" variant="subtle">
              {{ fanout.done }} of {{ fanout.total }} done
            </UBadge>
          </div>
          <div class="flex gap-1">
            <UTooltip
              v-if="inductionKind(level.kind)"
              :text="level.kind === 'category' ? (categoryBlocker() ?? '') : ''"
            >
              <UButton
                size="md"
                color="neutral"
                variant="outline"
                icon="i-lucide-sparkles"
                :loading="busy"
                :disabled="level.kind === 'category' && categoryBlocker() != null"
                @click="() => void induce(inductionKind(level.kind) ?? '', level.kind)"
              >
                Propose
              </UButton>
            </UTooltip>
            <UTooltip
              v-if="level.kind === 'category'"
              :text="
                parentsReady.length === 0
                  ? `No category has ${inductionMin} classified rows yet`
                  : `${parentsReady.length} categor${parentsReady.length === 1 ? 'y' : 'ies'} with enough rows`
              "
            >
              <UButton
                size="md"
                color="neutral"
                variant="outline"
                icon="i-lucide-sparkles"
                :loading="busy"
                :disabled="parentsReady.length === 0"
                @click="() => void induceAllSubcategories()"
              >
                Propose all subcategories
              </UButton>
            </UTooltip>
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
            <UButton
              v-if="level.kind === 'category'"
              size="md"
              color="error"
              variant="outline"
              icon="i-lucide-eraser"
              :disabled="busy"
              @click="() => void clearAll()"
            >
              Clear all
            </UButton>
          </div>
        </div>

        <ul v-if="linesFor(level.kind).length > 0" class="flex flex-col gap-1">
          <li
            v-for="line in linesFor(level.kind)"
            :key="`${line.label}:${line.runId ?? 'x'}`"
            class="flex flex-wrap items-center gap-2 text-sm"
          >
            <UIcon
              v-if="line.status === 'running'"
              name="i-lucide-loader-circle"
              class="size-4 animate-spin text-muted"
            />
            <UIcon
              v-else-if="line.status === 'completed'"
              name="i-lucide-check"
              class="size-4 text-success"
            />
            <UIcon
              v-else-if="line.status === 'undone'"
              name="i-lucide-rotate-ccw"
              class="size-4 text-muted"
            />
            <UIcon v-else name="i-lucide-circle-alert" class="size-4 text-warning" />
            <span class="text-highlighted">{{ line.label }}</span>
            <span v-if="line.runId != null" class="text-muted">· run #{{ line.runId }}</span>
            <span class="text-muted">· {{ line.text }}</span>
            <UButton
              v-if="line.status === 'completed'"
              size="md"
              color="neutral"
              variant="ghost"
              icon="i-lucide-rotate-ccw"
              :loading="undoing === line.runId"
              @click="() => void undoRun(line)"
            >
              Undo run
            </UButton>
          </li>
        </ul>

        <p v-if="roots(level.kind).length === 0" class="text-sm text-muted">
          Nothing yet<template v-if="inductionKind(level.kind)">
            — propose a list from summaries, or add by hand</template
          >.
        </p>

        <ul v-if="roots(level.kind).length > 0 || level.induced" class="flex flex-col gap-2">
          <li
            v-for="row in rootRows(level.kind)"
            :key="row.entry.id"
            class="rounded-lg border border-default bg-elevated p-3"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="flex min-w-0 flex-col gap-1">
                <div class="flex flex-wrap items-center gap-2">
                  <!-- Row header carries the entry's own buttons, so the chevron is
                       the trigger rather than the whole row (the same reason
                       CollapsibleSection keeps its header outside UCollapsible). -->
                  <UButton
                    v-if="level.childKind"
                    size="md"
                    color="neutral"
                    variant="ghost"
                    :icon="
                      isExpanded(row.entry) ? 'i-lucide-chevron-down' : 'i-lucide-chevron-right'
                    "
                    :aria-label="isExpanded(row.entry) ? 'Collapse' : 'Expand'"
                    @click="toggleExpanded(row.entry)"
                  />
                  <span class="text-sm font-medium text-highlighted">{{ row.entry.name }}</span>
                  <UIcon
                    v-if="row.entry.frozen && !isOtherEntry(row.entry)"
                    name="i-lucide-lock"
                    class="size-4 text-muted"
                  />
                  <span v-if="row.rows != null" class="text-sm text-muted">
                    {{ row.rows.toLocaleString() }} rows<template v-if="row.share != null">
                      · {{ pct(row.share) }}</template
                    >
                  </span>
                  <UBadge
                    v-if="row.badge"
                    size="md"
                    variant="subtle"
                    :color="badgeColor(row.badge.kind)"
                  >
                    {{ row.badge.text }}
                  </UBadge>
                  <UBadge
                    v-else-if="isOtherEntry(row.entry) && (row.share ?? 0) > 0.15"
                    size="md"
                    variant="subtle"
                    color="warning"
                  >
                    above 15% — the vocabulary misses these
                  </UBadge>
                </div>
                <template v-if="!level.childKind || isExpanded(row.entry)">
                  <p v-if="isOtherEntry(row.entry)" class="line-clamp-2 text-sm text-muted">
                    {{ OTHER_DEFINITION }}
                  </p>
                  <p v-else-if="row.entry.description" class="line-clamp-2 text-sm text-muted">
                    {{ row.entry.description }}
                  </p>
                  <p v-if="auditLine(row.entry)" class="text-sm text-dimmed">
                    {{ auditLine(row.entry) }}
                  </p>
                </template>
              </div>
              <div class="flex shrink-0 gap-1">
                <UTooltip
                  v-if="level.kind === 'category'"
                  :text="subcategoryBlocker(row.entry) ?? ''"
                >
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-sparkles"
                    :disabled="busy || subcategoryBlocker(row.entry) != null"
                    @click="() => void induce('propose_subcategories', level.kind, row.entry)"
                  >
                    Propose subcategories
                  </UButton>
                </UTooltip>
                <UButton
                  v-if="level.childKind"
                  size="md"
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-plus"
                  aria-label="Add child"
                  :disabled="busy"
                  @click="() => void define(level.childKind ?? level.kind, row.entry)"
                />
                <template v-if="!isOtherEntry(row.entry)">
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    :icon="row.entry.frozen ? 'i-lucide-lock-open' : 'i-lucide-lock'"
                    :aria-label="row.entry.frozen ? 'Unfreeze' : 'Freeze'"
                    :disabled="busy"
                    @click="() => void toggleFrozen(row.entry)"
                  />
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-pencil"
                    aria-label="Rename"
                    :disabled="busy || row.entry.frozen"
                    @click="() => void rename(row.entry)"
                  />
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-file-pen"
                    aria-label="Redefine"
                    :disabled="busy || row.entry.frozen"
                    @click="() => void redefine(row.entry)"
                  />
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-trash-2"
                    aria-label="Delete"
                    :disabled="busy || row.entry.frozen"
                    @click="() => void remove(row.entry)"
                  />
                </template>
              </div>
            </div>

            <p
              v-if="
                level.kind === 'category' &&
                isExpanded(row.entry) &&
                children(row.entry, 'subcategory').length === 0
              "
              class="mt-2 text-sm text-muted"
            >
              No subcategories — propose from the {{ rowsUnder(row.entry).toLocaleString() }} rows
              classified here, or add by hand.
            </p>
            <ul
              v-if="
                level.childKind &&
                isExpanded(row.entry) &&
                (children(row.entry, level.childKind).length > 0 || level.kind === 'category')
              "
              class="mt-2 flex flex-col gap-1 border-l border-default pl-3"
            >
              <li
                v-for="child in childRows(row.entry, level.childKind)"
                :key="child.entry.id"
                class="flex items-start justify-between gap-3"
              >
                <div class="flex min-w-0 flex-col">
                  <span class="flex flex-wrap items-center gap-2 text-sm text-highlighted">
                    {{ child.entry.name }}
                    <UIcon
                      v-if="child.entry.frozen"
                      name="i-lucide-lock"
                      class="size-3 text-muted"
                    />
                    <span v-if="child.rows != null" class="text-muted">
                      {{ child.rows.toLocaleString() }} rows<template v-if="child.share != null">
                        · {{ pct(child.share) }}</template
                      >
                    </span>
                    <UBadge
                      v-if="child.badge"
                      size="md"
                      variant="subtle"
                      :color="badgeColor(child.badge.kind)"
                    >
                      {{ child.badge.text }}
                    </UBadge>
                  </span>
                  <span v-if="child.entry.description" class="line-clamp-1 text-sm text-muted">
                    {{ child.entry.description }}
                  </span>
                  <span v-if="auditLine(child.entry)" class="text-sm text-dimmed">
                    {{ auditLine(child.entry) }}
                  </span>
                </div>
                <div class="flex shrink-0 gap-1">
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-pencil"
                    aria-label="Rename"
                    :disabled="busy || child.entry.frozen"
                    @click="() => void rename(child.entry)"
                  />
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-file-pen"
                    aria-label="Redefine"
                    :disabled="busy || child.entry.frozen"
                    @click="() => void redefine(child.entry)"
                  />
                  <UButton
                    size="md"
                    color="neutral"
                    variant="ghost"
                    icon="i-lucide-trash-2"
                    aria-label="Delete"
                    :disabled="busy || child.entry.frozen"
                    @click="() => void remove(child.entry)"
                  />
                </div>
              </li>
              <li v-if="level.kind === 'category'" class="flex items-start justify-between gap-3">
                <div class="flex min-w-0 flex-col">
                  <span class="flex items-center gap-2 text-sm text-highlighted">
                    other
                    <UBadge
                      v-if="childOtherRate(row.entry) != null"
                      size="md"
                      variant="subtle"
                      :color="(childOtherRate(row.entry) ?? 0) > 0.15 ? 'warning' : 'neutral'"
                    >
                      {{ pct(childOtherRate(row.entry) ?? 0) }} of rows
                    </UBadge>
                  </span>
                  <span class="line-clamp-1 text-sm text-muted">{{ OTHER_DEFINITION }}</span>
                </div>
              </li>
            </ul>
          </li>
          <!-- Levels without children keep the fixed `other` row; categories
               render it as an entry in the sorted list above. -->
          <li
            v-if="level.induced && !level.childKind"
            class="rounded-lg border border-dashed border-default p-3"
          >
            <div class="flex min-w-0 flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium text-highlighted">other</span>
                <UBadge
                  v-if="rootOtherRate(level.kind) != null"
                  size="md"
                  variant="subtle"
                  :color="(rootOtherRate(level.kind) ?? 0) > 0.15 ? 'warning' : 'neutral'"
                >
                  {{ pct(rootOtherRate(level.kind) ?? 0) }} of rows
                </UBadge>
              </div>
              <p class="text-sm text-muted">{{ OTHER_DEFINITION }}</p>
            </div>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>
