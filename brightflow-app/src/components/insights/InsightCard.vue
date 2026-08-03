<script setup lang="ts">
import { ChevronDown, ChevronRight, ExternalLink, Info } from '@lucide/vue';
import type { ContextMenuItem } from '@nuxt/ui';
import { computed, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { useInsightActions } from '@/composables/useInsightActions';
import { usePromptAction } from '@/composables/usePromptAction';
import { useInsightsStore } from '@/stores/insights';
import { useQueryStore } from '@/stores/query';
import type { AnalysisNode, AnalysisTree, DismissReason } from '@/types/generated';
import { displaySegmentValue, humanizeColumn } from '@/utils/format';

import { ANALYSIS_TYPE_META, dimensionOf, directionOf, qualitativeScore } from './nodeMeta';
import { rendererFor } from './renderers';

const props = defineProps<{
  node: AnalysisNode;
  tree: AnalysisTree;
  depth?: number;
}>();

const router = useRouter();
const route = useRoute();
const queryStore = useQueryStore();
const insightsStore = useInsightsStore();
const showDetails = ref(false);

function openInExplore(): void {
  const sourceId = insightsStore.selectedSourceId ?? String(route.params['sourceId'] ?? '');
  const table = insightsStore.selectedTable ?? String(route.params['table'] ?? '');
  if (!sourceId || !table) {
    return;
  }
  // Reset query state and apply this finding's filter chain
  queryStore.reset();
  queryStore.sections.filter.enabled = true;
  for (const step of props.node.filterChain) {
    queryStore.addFilter();
    const newest = queryStore.filters.at(-1);
    if (newest) {
      queryStore.updateFilter(newest.id, {
        column: step.column,
        op: 'eq',
        value: step.value,
      });
    }
  }
  router.push({ name: 'explore-table', params: { sourceId, table } });
}

const expanded = ref(false);
const currentDepth = computed(() => props.depth ?? 0);

const childNodes = computed(() =>
  props.node.children
    .map((childId) => props.tree.nodes.find((n) => n.id === childId))
    .filter((n): n is AnalysisNode => n !== undefined)
    .toSorted((a, b) => b.significance - a.significance),
);

const hasChildren = computed(() => childNodes.value.length > 0);

const renderer = computed(() => rendererFor(props.node.analysis.type));

// ── Curation actions (dismiss / pin / annotate / suppress) ────────────────
const actions = useInsightActions();
const prompt = usePromptAction();

function actionScope(): { sourceId: string; table: string } | null {
  const sourceId = insightsStore.selectedSourceId ?? String(route.params['sourceId'] ?? '');
  const table = insightsStore.selectedTable ?? String(route.params['table'] ?? '');
  if (!sourceId || !table || !props.node.fingerprint) {
    return null;
  }
  return { sourceId, table };
}

async function dismiss(reason: DismissReason): Promise<void> {
  const scope = actionScope();
  if (!scope) {
    return;
  }
  await actions.dismiss(scope, props.node.fingerprint, reason);
}

async function pin(): Promise<void> {
  const scope = actionScope();
  if (!scope) {
    return;
  }
  await actions.pin(scope, props.node.fingerprint);
}

async function annotate(): Promise<void> {
  const scope = actionScope();
  if (!scope) {
    return;
  }
  const note = await prompt('Note for this insight', { placeholder: 'Your note…' });
  if (note == null) {
    return;
  }
  await actions.annotate(scope, props.node.fingerprint, note);
}

async function suppressDimension(): Promise<void> {
  const scope = actionScope();
  if (!scope) {
    return;
  }
  const fromNode = dimensionOf(props.node);
  const target = fromNode ?? (await prompt('Column to suppress', { placeholder: 'Column name' }));
  if (target == null) {
    return;
  }
  await actions.suppressSegment(scope, target);
}

const isPinned = computed(
  () => props.node.fingerprint !== '' && insightsStore.overlay.pinned.has(props.node.fingerprint),
);
// Session-scoped: notes made in this session show a badge. Backend follow-up:
// Return stored annotations with the tree so they survive reloads.
const note = computed(() =>
  props.node.fingerprint === ''
    ? undefined
    : insightsStore.overlay.notes.get(props.node.fingerprint),
);

const curationItems = computed<ContextMenuItem[][]>(() => [
  [
    {
      label: 'Pin to top',
      icon: 'i-lucide-pin',
      kbds: ['meta', 'P'],
      onSelect: () => void pin(),
    },
    { label: 'Annotate…', icon: 'i-lucide-message-square-text', onSelect: () => void annotate() },
  ],
  [
    {
      label: 'Dismiss — boring',
      icon: 'i-lucide-eye-off',
      onSelect: () => void dismiss('boring'),
    },
    { label: 'Dismiss — known', icon: 'i-lucide-eye-off', onSelect: () => void dismiss('known') },
    { label: 'Dismiss — wrong', icon: 'i-lucide-eye-off', onSelect: () => void dismiss('wrong') },
  ],
  [
    {
      label: 'Suppress this segment',
      icon: 'i-lucide-ban',
      onSelect: () => void suppressDimension(),
    },
  ],
]);

// ⌘P only pins the hovered insight; the config is empty while not hovered so the
// global listener doesn't fire (or preventDefault) elsewhere. Gated to
// Top-level cards because nested cards share hover with their ancestor —
// Right-click (context menu) covers the nested case unambiguously.
const isHovered = ref(false);
defineShortcuts(
  computed(() =>
    isHovered.value && currentDepth.value === 0 && props.node.fingerprint
      ? extractShortcuts(curationItems.value)
      : {},
  ),
);

const typeMeta = computed(() => ANALYSIS_TYPE_META[props.node.analysis.type]);
const scoreTier = computed(() => qualitativeScore(props.node.significance));

// Direction arrow, colored by measure polarity when the backend tagged one:
// Good news green, bad news red, untagged neutral.
const direction = computed(() => directionOf(props.node));
const SENTIMENT_META = {
  bad: { color: 'text-error', title: 'Moving in the wrong direction for this measure' },
  good: { color: 'text-success', title: 'Moving in the right direction for this measure' },
} as const;

const directionMeta = computed(() => {
  const dir = direction.value;
  if (dir == null) {
    return null;
  }
  const sentiment = props.node.sentiment;
  const meta = sentiment == null ? undefined : SENTIMENT_META[sentiment];
  return {
    color: meta?.color ?? 'text-muted',
    icon: dir === 'up' ? 'i-lucide-arrow-up-right' : 'i-lucide-arrow-down-right',
    title: meta?.title ?? `Direction: ${dir}`,
  };
});

const drillPath = computed(() =>
  props.node.filterChain
    .map((s) => `${humanizeColumn(s.column)} = ${displaySegmentValue(s.value)}`)
    .join(' › '),
);

/** Human labels + explanations for the score components. */
const scoreRows = computed(() => [
  {
    hint: 'How unlikely this pattern is under the “nothing happened” assumption — closer to 1 means harder to explain away as noise.',
    label: 'Statistical strength',
    value: props.node.scoreBreakdown.significance.toFixed(3),
  },
  {
    hint: 'Share of the table’s rows this finding covers.',
    label: 'Data affected',
    value: props.node.scoreBreakdown.impact.toFixed(3),
  },
  {
    hint: 'Fresh stories score 1.0; ones you have seen in past runs decay toward 0.',
    label: 'Freshness',
    value: props.node.scoreBreakdown.novelty.toFixed(2),
  },
  {
    hint: 'Findings about columns you marked as KPIs get boosted.',
    label: 'KPI weight',
    value: `×${props.node.scoreBreakdown.kpiBoost.toFixed(2)}`,
  },
]);
</script>

<template>
  <UContextMenu :items="curationItems" :disabled="!node.fingerprint">
    <div
      class="rounded-lg border transition-colors"
      :class="[
        currentDepth === 0 ? 'border-default bg-default' : 'border-default/50 bg-elevated/50',
        currentDepth > 0 ? 'ml-6' : '',
      ]"
      @mouseenter="isHovered = true"
      @mouseleave="isHovered = false"
    >
      <!-- Card header -->
      <button
        class="flex w-full items-start gap-3 p-4 text-left"
        :class="{ 'cursor-pointer hover:bg-elevated/50': hasChildren }"
        @click="hasChildren ? (expanded = !expanded) : undefined"
      >
        <!-- Type icon -->
        <div class="flex-shrink-0 rounded-md p-1.5" :class="typeMeta.bg">
          <component :is="typeMeta.icon" class="h-4 w-4" :class="typeMeta.color" />
        </div>

        <!-- Content -->
        <div class="min-w-0 flex-1">
          <!-- Summary -->
          <p class="text-sm leading-relaxed text-highlighted">
            <span
              v-if="node.rank != null && currentDepth === 0"
              class="mr-1.5 font-mono-data text-xs text-muted"
              >#{{ node.rank }}</span
            >
            <UIcon
              v-if="isPinned"
              name="i-lucide-pin"
              class="mr-1 inline-block size-3.5 text-primary"
            /><UIcon
              v-if="directionMeta"
              :name="directionMeta.icon"
              class="mr-1 inline-block size-3.5"
              :class="directionMeta.color"
              :title="directionMeta.title"
            />{{ node.summary }}
          </p>
          <!-- Why this matters, in plain language -->
          <p v-if="node.why" class="mt-1 text-sm text-muted">{{ node.why }}</p>
          <!-- Provenance chips: how the underlying series was derived -->
          <div
            v-if="node.provenance.length > 0 || note != null"
            class="mt-1.5 flex flex-wrap items-center gap-1"
          >
            <span
              v-for="(step, i) in node.provenance"
              :key="i"
              class="rounded border border-default bg-elevated/60 px-1.5 py-0.5 font-mono-data text-xs text-muted"
              :title="step.kind"
            >
              {{ step.label }}
            </span>
            <UTooltip v-if="note != null" :text="note">
              <span
                class="inline-flex items-center gap-1 rounded border border-primary-500/30 bg-primary-500/10 px-1.5 py-0.5 text-xs text-primary"
              >
                <UIcon name="i-lucide-message-square-text" class="size-3" />
                Note
              </span>
            </UTooltip>
          </div>
        </div>

        <!-- Expand/collapse indicator -->
        <div v-if="hasChildren" class="mt-0.5 flex-shrink-0">
          <span class="mr-1 text-xs text-muted">{{ childNodes.length }}</span>
          <component
            :is="expanded ? ChevronDown : ChevronRight"
            class="inline h-4 w-4 text-muted"
          />
        </div>
      </button>

      <!-- Per-type chart renderer -->
      <div v-if="renderer && node.data" class="px-4 pb-3">
        <component :is="renderer" :node="node" />
      </div>

      <!-- Footer: actions + details toggle -->
      <div class="flex items-center gap-2 px-4 pb-3 text-xs text-muted">
        <button
          class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-elevated hover:text-highlighted"
          @click.stop="showDetails = !showDetails"
        >
          <Info class="h-3 w-3" />
          Details
        </button>
        <button
          v-if="node.filterChain.length > 0"
          class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-elevated hover:text-highlighted"
          @click.stop="openInExplore"
        >
          <ExternalLink class="h-3 w-3" />
          Open in Explore
        </button>
        <UTooltip :text="`Composite score ${node.significance.toFixed(2)}`" class="ml-auto">
          <span class="rounded-full border border-default px-2 py-0.5">
            {{ scoreTier.label }}
          </span>
        </UTooltip>
        <UDropdownMenu v-if="node.fingerprint" :items="curationItems">
          <UButton
            size="xs"
            color="neutral"
            variant="ghost"
            icon="i-lucide-more-horizontal"
            aria-label="Insight actions"
            @click.stop
          />
        </UDropdownMenu>
      </div>

      <div
        v-if="showDetails"
        class="mx-4 mb-3 rounded-md border border-default bg-elevated/40 p-3 text-sm text-muted"
      >
        <p class="mb-2 font-mono-data">{{ node.techSummary }}</p>
        <div class="grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-4">
          <UTooltip v-for="row in scoreRows" :key="row.label" :text="row.hint">
            <div>
              <div class="text-xs tracking-wider uppercase">{{ row.label }}</div>
              <div class="font-mono-data text-highlighted">{{ row.value }}</div>
            </div>
          </UTooltip>
        </div>
        <div class="mt-2 border-t border-default pt-2">
          Composite score:
          <span class="font-mono-data text-highlighted">{{ node.significance.toFixed(2) }}</span>
          ({{ scoreTier.label.toLowerCase() }})
        </div>
        <div v-if="node.filterChain.length > 0" class="mt-2 border-t border-default pt-2">
          Drill path: {{ drillPath }}
        </div>
      </div>

      <!-- Children (drill-down) -->
      <div v-if="expanded && hasChildren" class="space-y-2 px-4 pb-4">
        <InsightCard
          v-for="child in childNodes"
          :key="child.id"
          :node="child"
          :tree="tree"
          :depth="currentDepth + 1"
        />
      </div>
    </div>
  </UContextMenu>
</template>
