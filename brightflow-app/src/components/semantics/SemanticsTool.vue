<script setup lang="ts">
/**
 * The Semantics page: one table, one row per column, showing what every
 * reader resolves — type, role, KPI, polarity, label, description — and who
 * said it, with the opinion rows behind each column one click away. The
 * table's own settings and doc columns sit above the columns; what each
 * producer's re-declarations changed is listed below.
 *
 * Edits are the same actions the Explore column menu and the palette
 * dispatch, so this is a view with the existing edits attached, not a
 * second editing path. Server data comes through Pinia Colada and is
 * refetched on every applied or undone action that names this table: a
 * change may reveal what a lower layer says, which no event carries.
 *
 * The source's model import and export sits at the foot, collapsed: it is
 * source-scoped, so it shows whether or not a table is picked.
 *
 * A table that is a model's output gets a Recipe section between the table
 * and its columns: what it is built from, the chain in words, the last
 * build, and rebuild / edit / delete through the same bus as everything
 * else here.
 */

import type { DropdownMenuItem } from '@nuxt/ui';
import { useQuery } from '@pinia/colada';
import { computed, onBeforeUnmount, ref } from 'vue';
import { useRouter } from 'vue-router';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { columnMenuItems } from '@/components/query/columnMenu';
import SemanticModelPanel from '@/components/semantics/SemanticModelPanel.vue';
import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { relativeTime } from '@/components/tools/overview';
import { useColumnSemantics } from '@/composables/useColumnSemantics';
import { useInsightActions } from '@/composables/useInsightActions';
import { useModels } from '@/composables/useModels';
import { usePromptAction } from '@/composables/usePromptAction';
import { semanticsApi } from '@/services/api';
import { isActionEvent } from '@/services/wsGuards';
import { useConnectionStore } from '@/stores/connection';
import type { SourceTable } from '@/types';
import type { Layer, ResolvedColumn, TimeGranularity } from '@/types/generated';
import { changeLines, changeSummary } from '@/utils/declarationChanges';
import { describeOperation } from '@/utils/modelRecipe';
import { POLARITY_LABELS, provenanceLabel, ROLE_LABELS } from '@/utils/semanticLabels';

import {
  columnOpinionFields,
  docSummary,
  LAYER_LABELS,
  opinionsFor,
  producerText,
  sortOpinions,
  tableOpinionFields,
  toColumnInfo,
} from './semanticsView';

const props = defineProps<{
  sourceId: string;
  table?: string | undefined;
}>();

const router = useRouter();
const connection = useConnectionStore();
const semantics = useColumnSemantics();
const { dispatchWithFeedback } = useInsightActions();
const promptText = usePromptAction();

const hasTable = computed(() => props.table != null && props.table !== '');
const tableName = computed(() => props.table ?? '');

const modelList = useModels(() => props.sourceId);
/** The model this table is the output of, if it is one. */
const thisModel = computed(() => modelList.forTable(tableName.value) ?? null);
const recipeSteps = computed(() =>
  thisModel.value == null
    ? []
    : thisModel.value.recipe.operations.map((op) => describeOperation(op)),
);
const lastBuildText = computed(() => {
  const build = thisModel.value?.lastBuild;
  if (build == null) {
    return 'never built';
  }
  const ago = relativeTime(new Date(build.startedAt * 1000));
  if (build.status === 'failed') {
    return `failed ${ago}: ${build.error ?? 'unknown error'}`;
  }
  if (build.status === 'running') {
    return `building, started ${ago}`;
  }
  return `${build.rows ?? 0} rows, built ${ago}`;
});

async function editRecipe(): Promise<void> {
  const model = thisModel.value;
  if (model?.inputTable == null) {
    return;
  }
  await router.push({
    name: 'explore-table',
    params: { sourceId: props.sourceId, table: model.inputTable },
    query: { model: model.id },
  });
}

async function removeModel(): Promise<void> {
  const model = thisModel.value;
  if (model == null) {
    return;
  }
  await modelList.remove(model);
  await router.push({
    name: 'source-tool',
    params: { sourceId: props.sourceId, tool: 'semantics' },
  });
}

const { data: columnsData, refetch: refetchColumns } = useQuery({
  key: () => ['semantics-columns', props.sourceId, tableName.value],
  query: async () => await semanticsApi.columns(props.sourceId, tableName.value),
  enabled: () => hasTable.value,
});
const { data: tableData, refetch: refetchTable } = useQuery({
  key: () => ['semantics-table', props.sourceId, tableName.value],
  query: async () => await semanticsApi.table(props.sourceId, tableName.value),
  enabled: () => hasTable.value,
});
const { data: changesData, refetch: refetchChanges } = useQuery({
  key: () => ['semantics-changes', props.sourceId, tableName.value],
  query: async () => await semanticsApi.changes(props.sourceId, tableName.value),
  enabled: () => hasTable.value,
});

const columns = computed(() => columnsData.value?.columns ?? []);
const columnLayers = computed(() => columnsData.value?.layers ?? []);
const resolvedTable = computed(() => tableData.value?.table ?? null);
const tableLayers = computed(() => sortOpinions(tableData.value?.layers ?? []));
const changes = computed(() => changesData.value?.changes ?? []);

async function refetchAll(): Promise<void> {
  await Promise.all([refetchColumns(), refetchTable(), refetchChanges()]);
}

// An applied or undone action on this table changes what resolves. A
// Proposal does not, so the page ignores it until approval.
const stopEvents = connection.onMessage('actionEvent', (payload) => {
  if (!isActionEvent(payload)) {
    return;
  }
  const { entry } = payload;
  const params = entry.params as { table?: unknown } | null;
  if (params?.table === props.table && (entry.status === 'applied' || entry.status === 'undone')) {
    void refetchAll();
  }
});
onBeforeUnmount(stopEvents);

function handleSelectTable(table: SourceTable): void {
  router.push({ name: 'semantics-table', params: { sourceId: props.sourceId, table: table.name } });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({
    name: 'semantics-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

// ── Column rows ──────────────────────────────────────────────────────────────

const expanded = ref<Set<string>>(new Set());

function toggle(name: string): void {
  const next = new Set(expanded.value);
  if (next.has(name)) {
    next.delete(name);
  } else {
    next.add(name);
  }
  expanded.value = next;
}

function shownName(column: ResolvedColumn): string {
  return column.label == null || column.label === '' ? column.name : column.label;
}

function sourceLabel(column: ResolvedColumn): string {
  return column.resolved_by == null ? '—' : provenanceLabel(column.resolved_by);
}

/** The Explore column menu, as a dropdown, with a refetch after each edit. */
function menuFor(column: ResolvedColumn): DropdownMenuItem[][] {
  const info = toColumnInfo(column);
  const scope = { sourceId: props.sourceId, table: tableName.value };
  const then = (edit: Promise<void>): void => {
    void edit.then(refetchAll);
  };
  return columnMenuItems(info, {
    clearDescription: () => then(semantics.clearDescription(scope, info)),
    clearLabel: () => then(semantics.clearLabel(scope, info)),
    describe: () => then(semantics.describe(scope, info)),
    rename: () => then(semantics.rename(scope, info)),
    reset: () => then(semantics.reset(scope, info)),
    setKpi: (isKpi) => then(semantics.setKpi(scope, info, isKpi)),
    setPolarity: (polarity) => then(semantics.setPolarity(scope, info, polarity)),
    setRole: (role) => then(semantics.setRole(scope, info, role)),
  });
}

const LAYER_COLOR: Record<Layer, 'primary' | 'info' | 'neutral'> = {
  agent: 'info',
  declared: 'neutral',
  detected: 'neutral',
  user: 'primary',
};

function when(updatedAt: bigint | number): string {
  const seconds = Number(updatedAt);
  return seconds > 0 ? new Date(seconds * 1000).toLocaleString() : '';
}

// ── Table settings ───────────────────────────────────────────────────────────

const GRANULARITY_OPTIONS: { label: string; value: TimeGranularity }[] = [
  { label: 'Day', value: 'day' },
  { label: 'Week', value: 'week' },
  { label: 'Month', value: 'month' },
  { label: 'Quarter', value: 'quarter' },
  { label: 'Year', value: 'year' },
];

async function renameTable(): Promise<void> {
  const displayName = await promptText('Rename table', {
    confirmLabel: 'Rename',
    description: tableName.value,
    initialValue: resolvedTable.value?.display_name ?? '',
    placeholder: 'Shown instead of the table name',
  });
  if (displayName == null) {
    return;
  }
  await dispatchWithFeedback(
    {
      display_name: displayName,
      kind: 'set_table_settings',
      source_id: props.sourceId,
      table: tableName.value,
    },
    { description: `${tableName.value} → ${displayName}`, title: 'Table renamed' },
  );
  await refetchAll();
}

async function describeTable(): Promise<void> {
  const description = await promptText('Describe table', {
    confirmLabel: 'Save',
    description: tableName.value,
    initialValue: resolvedTable.value?.description ?? '',
    placeholder: 'What one row of this table is',
  });
  if (description == null) {
    return;
  }
  await dispatchWithFeedback(
    {
      description,
      kind: 'set_table_settings',
      source_id: props.sourceId,
      table: tableName.value,
    },
    { description: tableName.value, title: 'Description saved' },
  );
  await refetchAll();
}

async function setGranularity(granularity: TimeGranularity): Promise<void> {
  await dispatchWithFeedback(
    {
      kind: 'set_table_settings',
      source_id: props.sourceId,
      table: tableName.value,
      time_granularity: granularity,
    },
    { description: `${tableName.value}: by ${granularity}`, title: 'Analysis period set' },
  );
  await refetchAll();
}

const granularityMenu = computed<DropdownMenuItem[]>(() =>
  GRANULARITY_OPTIONS.map((option) => ({
    checked: resolvedTable.value?.time_granularity === option.value,
    label: option.label,
    onSelect: () => void setGranularity(option.value),
    type: 'checkbox',
  })),
);

const tableSettingsMenu = computed<DropdownMenuItem[][]>(() => [
  [
    { icon: 'i-lucide-pencil-line', label: 'Rename table…', onSelect: () => void renameTable() },
    { icon: 'i-lucide-text', label: 'Describe table…', onSelect: () => void describeTable() },
  ],
  [{ children: granularityMenu.value, icon: 'i-lucide-calendar-range', label: 'Analysis period' }],
]);

const tableLayersOpen = ref(false);
const changesOpen = ref(false);
const modelOpen = ref(false);
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto">
    <TableSectionPane
      :source-id="sourceId"
      :selected-table="table"
      @select-table="handleSelectTable"
      @auto-select-table="handleAutoSelectTable"
    />

    <div v-if="hasTable" class="space-y-6 p-4">
      <!-- The table itself -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Table</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0">
              <p class="text-sm font-medium text-highlighted">
                {{ resolvedTable?.display_name || tableName }}
                <span v-if="resolvedTable?.display_name" class="font-normal text-muted">
                  {{ tableName }}
                </span>
              </p>
              <p class="mt-1 text-base text-default">
                {{ resolvedTable?.description || 'No description yet.' }}
              </p>
              <dl class="mt-3 grid grid-cols-1 gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
                <div class="flex gap-2">
                  <dt class="text-muted">Analysis period</dt>
                  <dd class="text-default">{{ resolvedTable?.time_granularity ?? '—' }}</dd>
                </div>
                <div class="flex gap-2">
                  <dt class="text-muted">Periods compared</dt>
                  <dd class="text-default">{{ resolvedTable?.comparison_periods ?? '—' }}</dd>
                </div>
                <div class="flex gap-2 sm:col-span-2">
                  <dt class="text-muted">Doc columns</dt>
                  <dd class="text-default">
                    {{ resolvedTable?.doc ? docSummary(resolvedTable.doc) : '—' }}
                  </dd>
                </div>
                <div class="flex gap-2 sm:col-span-2">
                  <dt class="text-muted">Source</dt>
                  <dd class="text-default">
                    {{
                      resolvedTable?.resolved_by ? provenanceLabel(resolvedTable.resolved_by) : '—'
                    }}
                  </dd>
                </div>
              </dl>
            </div>
            <UDropdownMenu :items="tableSettingsMenu">
              <UButton size="md" color="neutral" variant="ghost" icon="i-lucide-pencil">
                Edit
              </UButton>
            </UDropdownMenu>
          </div>
          <div v-if="tableLayers.length > 0" class="mt-3 border-t border-default pt-3">
            <button
              type="button"
              class="flex items-center gap-1 text-sm text-muted hover:text-default"
              @click="tableLayersOpen = !tableLayersOpen"
            >
              <UIcon
                :name="tableLayersOpen ? 'i-lucide-chevron-down' : 'i-lucide-chevron-right'"
                class="h-4 w-4"
              />
              {{ tableLayers.length }} layer{{ tableLayers.length === 1 ? '' : 's' }}
            </button>
            <ul v-if="tableLayersOpen" class="mt-2 space-y-2">
              <li v-for="opinion in tableLayers" :key="producerText(opinion.provenance)">
                <div class="flex flex-wrap items-center gap-2 text-sm">
                  <UBadge size="md" variant="subtle" :color="LAYER_COLOR[opinion.provenance.layer]">
                    {{ LAYER_LABELS[opinion.provenance.layer] }}
                  </UBadge>
                  <span class="font-mono text-default">{{ producerText(opinion.provenance) }}</span>
                  <span class="text-muted">{{ when(opinion.updated_at) }}</span>
                </div>
                <ul class="mt-1 ml-1 space-y-0.5 text-sm">
                  <li v-for="stated in tableOpinionFields(opinion)" :key="stated.field">
                    <span class="text-muted">{{ stated.field }}:</span>
                    <span class="text-default">{{ stated.value }}</span>
                  </li>
                  <li v-if="tableOpinionFields(opinion).length === 0" class="text-muted">
                    Says nothing about the table's fields.
                  </li>
                </ul>
              </li>
            </ul>
          </div>
        </div>
      </section>

      <!-- Recipe: only on a model's output table -->
      <section v-if="thisModel">
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Recipe</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
              <p class="text-sm text-default">
                <template v-if="thisModel.inputTable">
                  Built from
                  <RouterLink
                    :to="{
                      name: 'semantics-table',
                      params: { sourceId, table: thisModel.inputTable },
                    }"
                    class="font-medium underline-offset-2 hover:underline"
                  >
                    {{ thisModel.inputTable }}
                  </RouterLink>
                </template>
                <template v-else>Input table deleted; the next build will fail.</template>
                <span class="text-muted"> · version {{ thisModel.version }}</span>
              </p>
              <ol class="mt-2 list-decimal space-y-0.5 pl-5 text-sm text-default">
                <li v-for="(step, i) in recipeSteps" :key="i">{{ step }}</li>
                <li v-if="recipeSteps.length === 0" class="text-muted">the whole table</li>
              </ol>
              <p class="mt-2 text-sm text-muted">Last build: {{ lastBuildText }}</p>
            </div>
            <div class="flex flex-shrink-0 items-center gap-1.5">
              <UButton
                size="md"
                color="neutral"
                variant="soft"
                icon="i-lucide-refresh-cw"
                @click="thisModel && modelList.rebuild(thisModel)"
              >
                Rebuild
              </UButton>
              <UButton
                v-if="thisModel.clientSpec != null && thisModel.inputTable != null"
                size="md"
                color="neutral"
                variant="soft"
                icon="i-lucide-pencil-line"
                @click="editRecipe"
              >
                Edit recipe
              </UButton>
              <UButton
                size="md"
                color="error"
                variant="soft"
                icon="i-lucide-trash-2"
                @click="removeModel"
              >
                Delete model
              </UButton>
            </div>
          </div>
        </div>
      </section>

      <!-- Columns -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Columns</h3>
        <div class="overflow-x-auto rounded-lg border border-default bg-elevated">
          <table v-if="columns.length > 0" class="w-full text-sm">
            <thead class="text-left text-muted">
              <tr class="border-b border-default">
                <th class="px-3 py-2 font-medium">Column</th>
                <th class="px-3 py-2 font-medium">Type</th>
                <th class="px-3 py-2 font-medium">Role</th>
                <th class="px-3 py-2 font-medium">Polarity</th>
                <th class="px-3 py-2 font-medium">Description</th>
                <th class="px-3 py-2 font-medium">Source</th>
                <th class="px-3 py-2"></th>
              </tr>
            </thead>
            <tbody>
              <template v-for="column in columns" :key="column.name">
                <tr class="border-b border-default last:border-b-0">
                  <td class="px-3 py-2 align-top">
                    <button
                      type="button"
                      class="flex items-start gap-1 text-left"
                      :aria-expanded="expanded.has(column.name)"
                      @click="toggle(column.name)"
                    >
                      <UIcon
                        :name="
                          expanded.has(column.name)
                            ? 'i-lucide-chevron-down'
                            : 'i-lucide-chevron-right'
                        "
                        class="mt-0.5 h-4 w-4 flex-shrink-0 text-muted"
                      />
                      <span class="min-w-0">
                        <span class="block text-highlighted">{{ shownName(column) }}</span>
                        <span
                          v-if="column.label && column.label !== column.name"
                          class="block font-mono text-muted"
                        >
                          {{ column.name }}
                        </span>
                      </span>
                    </button>
                  </td>
                  <td class="px-3 py-2 align-top font-mono text-muted">
                    {{ column.datatype ?? '—' }}
                    <span v-if="column.is_time" class="ml-1 font-sans">(time axis)</span>
                  </td>
                  <td class="px-3 py-2 align-top text-default">
                    {{ column.role ? ROLE_LABELS[column.role] : '—' }}
                    <UBadge v-if="column.is_kpi" size="md" variant="subtle" color="primary">
                      KPI
                    </UBadge>
                  </td>
                  <td class="px-3 py-2 align-top text-default">
                    {{
                      column.role === 'measure' && column.polarity
                        ? POLARITY_LABELS[column.polarity]
                        : '—'
                    }}
                  </td>
                  <td class="max-w-md px-3 py-2 align-top text-default" :title="column.description">
                    {{ column.description || '—' }}
                  </td>
                  <td class="px-3 py-2 align-top whitespace-nowrap text-muted">
                    {{ sourceLabel(column) }}
                  </td>
                  <td class="px-1 py-1 align-top">
                    <UDropdownMenu :items="menuFor(column)">
                      <UButton
                        size="md"
                        color="neutral"
                        variant="ghost"
                        icon="i-lucide-ellipsis"
                        :aria-label="`Edit ${column.name}`"
                      />
                    </UDropdownMenu>
                  </td>
                </tr>
                <tr v-if="expanded.has(column.name)" class="border-b border-default bg-muted/10">
                  <td colspan="7" class="px-3 py-2 pl-9">
                    <ul class="space-y-2">
                      <li
                        v-for="opinion in opinionsFor(columnLayers, column.name)"
                        :key="producerText(opinion.provenance)"
                      >
                        <div class="flex flex-wrap items-center gap-2 text-sm">
                          <UBadge
                            size="md"
                            variant="subtle"
                            :color="LAYER_COLOR[opinion.provenance.layer]"
                          >
                            {{ LAYER_LABELS[opinion.provenance.layer] }}
                          </UBadge>
                          <span class="font-mono text-default">
                            {{ producerText(opinion.provenance) }}
                          </span>
                          <span class="text-muted">{{ when(opinion.updated_at) }}</span>
                        </div>
                        <ul class="mt-1 ml-1 space-y-0.5 text-sm">
                          <li v-for="stated in columnOpinionFields(opinion)" :key="stated.field">
                            <span class="text-muted">{{ stated.field }}:</span>
                            <span class="text-default">{{ stated.value }}</span>
                          </li>
                          <li v-if="columnOpinionFields(opinion).length === 0" class="text-muted">
                            Says nothing about this column's fields.
                          </li>
                        </ul>
                      </li>
                      <li
                        v-if="opinionsFor(columnLayers, column.name).length === 0"
                        class="text-sm text-muted"
                      >
                        No layer has an opinion about this column.
                      </li>
                    </ul>
                  </td>
                </tr>
              </template>
            </tbody>
          </table>
          <p v-else class="p-4 text-sm text-muted">
            No semantics for this table yet. The detector runs when a table is created; a
            connector's declaration, an agent run or an edit in Explore adds to it.
          </p>
        </div>
      </section>

      <!-- Producer changes -->
      <section v-if="changes.length > 0">
        <CollapsibleSection v-model:open="changesOpen" class="rounded-lg border border-default">
          <template #title>
            <h3 class="text-sm font-medium text-default">
              Declaration changes
              <span class="font-normal text-muted">({{ changes.length }})</span>
            </h3>
          </template>
          <ul class="divide-y divide-default border-t border-default">
            <li v-for="(diff, index) in changes" :key="index" class="px-4 py-3">
              <p class="text-sm text-highlighted">
                <span class="font-mono">{{ diff.producer }}</span> · {{ changeSummary(diff) }}
              </p>
              <ul class="mt-1 space-y-0.5 font-mono text-sm text-muted">
                <li v-for="line in changeLines(diff)" :key="line">{{ line }}</li>
              </ul>
            </li>
          </ul>
        </CollapsibleSection>
      </section>
    </div>

    <!-- The whole source's model, in and out -->
    <div class="px-4 pb-4" :class="{ 'pt-4': !hasTable }">
      <CollapsibleSection v-model:open="modelOpen" class="rounded-lg border border-default">
        <template #title>
          <h3 class="text-sm font-medium text-default">
            Semantic model
            <span class="font-normal text-muted">import and export, whole source</span>
          </h3>
        </template>
        <div class="border-t border-default p-4">
          <SemanticModelPanel :source-id="sourceId" />
        </div>
      </CollapsibleSection>
    </div>
  </div>
</template>
