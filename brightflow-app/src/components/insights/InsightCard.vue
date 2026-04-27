<script setup lang="ts">
import {
  AlertTriangle,
  ArrowLeftRight,
  BarChart3,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  GitBranch,
  Info,
  PieChart,
  Sigma,
  Split,
  Target,
  TrendingUp,
  Users,
  Waves,
} from 'lucide-vue-next';
import { computed, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import type { AnalysisNode, AnalysisTree } from '@/services/api';
import { useInsightsStore } from '@/stores/insights';
import { useQueryStore } from '@/stores/query';

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
const showScore = ref(false);

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

const analysisTypeConfig = computed(() => {
  const { type } = props.node.analysis;
  switch (type) {
    case 'Anomaly': {
      return { bg: 'bg-red-500/10', color: 'text-red-500', icon: AlertTriangle, label: 'Anomaly' };
    }
    case 'Trend': {
      return {
        bg: 'bg-blue-500/10',
        color: 'text-blue-500',
        icon: TrendingUp,
        label: 'Trend',
      };
    }
    case 'PeriodComparison': {
      return {
        bg: 'bg-purple-500/10',
        color: 'text-purple-500',
        icon: ArrowLeftRight,
        label: 'Period',
      };
    }
    case 'PeriodAnomaly': {
      return {
        bg: 'bg-orange-500/10',
        color: 'text-orange-500',
        icon: AlertTriangle,
        label: 'Period Anomaly',
      };
    }
    case 'Seasonality': {
      return {
        bg: 'bg-teal-500/10',
        color: 'text-teal-500',
        icon: Waves,
        label: 'Seasonality',
      };
    }
    case 'Segment': {
      return {
        bg: 'bg-indigo-500/10',
        color: 'text-indigo-500',
        icon: BarChart3,
        label: 'Segment',
      };
    }
    case 'Correlation': {
      return {
        bg: 'bg-pink-500/10',
        color: 'text-pink-500',
        icon: GitBranch,
        label: 'Correlation',
      };
    }
    case 'ForecastDeviation': {
      return {
        bg: 'bg-amber-500/10',
        color: 'text-amber-500',
        icon: Target,
        label: 'Forecast',
      };
    }
    case 'OutlierCluster': {
      return {
        bg: 'bg-red-400/10',
        color: 'text-red-400',
        icon: AlertTriangle,
        label: 'Outlier Cluster',
      };
    }
    case 'Concentration': {
      return {
        bg: 'bg-violet-500/10',
        color: 'text-violet-500',
        icon: PieChart,
        label: 'Concentration',
      };
    }
    case 'DistributionShift': {
      return {
        bg: 'bg-cyan-500/10',
        color: 'text-cyan-500',
        icon: Split,
        label: 'Distribution Shift',
      };
    }
    case 'MembershipChange': {
      return {
        bg: 'bg-emerald-500/10',
        color: 'text-emerald-500',
        icon: Users,
        label: 'Membership',
      };
    }
    case 'ChangePoint': {
      return {
        bg: 'bg-rose-500/10',
        color: 'text-rose-500',
        icon: Sigma,
        label: 'Change Point',
      };
    }
  }
});
</script>

<template>
  <div
    class="rounded-lg border transition-colors"
    :class="[
      currentDepth === 0 ? 'border-default bg-default' : 'border-default/50 bg-elevated/50',
      currentDepth > 0 ? 'ml-6' : '',
    ]"
  >
    <!-- Card header -->
    <button
      class="flex w-full items-start gap-3 p-4 text-left"
      :class="{ 'cursor-pointer hover:bg-elevated/50': hasChildren }"
      @click="hasChildren ? (expanded = !expanded) : undefined"
    >
      <!-- Type icon -->
      <div class="flex-shrink-0 rounded-md p-1.5" :class="analysisTypeConfig.bg">
        <component
          :is="analysisTypeConfig.icon"
          class="h-4 w-4"
          :class="analysisTypeConfig.color"
        />
      </div>

      <!-- Content -->
      <div class="min-w-0 flex-1">
        <!-- Summary -->
        <p class="text-sm leading-relaxed text-highlighted">
          {{ node.summary }}
        </p>
        <!-- Tech summary badge -->
        <p class="mt-1 font-mono-data text-sm text-muted">
          {{ node.tech_summary }}
        </p>
      </div>

      <!-- Expand/collapse indicator -->
      <div v-if="hasChildren" class="mt-0.5 flex-shrink-0">
        <span class="mr-1 text-xs text-muted">{{ childNodes.length }}</span>
        <component :is="expanded ? ChevronDown : ChevronRight" class="inline h-4 w-4 text-muted" />
      </div>
    </button>

    <!-- Per-type chart renderer -->
    <div v-if="renderer && node.data" class="px-4 pb-3">
      <component :is="renderer" :node="node" />
    </div>

    <!-- Footer: actions + score breakdown -->
    <div class="flex items-center gap-2 px-4 pb-3 text-xs text-muted">
      <button
        class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-elevated hover:text-highlighted"
        @click.stop="showScore = !showScore"
      >
        <Info class="h-3 w-3" />
        Why this finding?
      </button>
      <button
        v-if="node.filterChain.length > 0"
        class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-elevated hover:text-highlighted"
        @click.stop="openInExplore"
      >
        <ExternalLink class="h-3 w-3" />
        Open in Explore
      </button>
      <span class="ml-auto font-mono-data">score {{ node.significance.toFixed(2) }}</span>
    </div>

    <div
      v-if="showScore"
      class="mx-4 mb-3 rounded-md border border-default bg-elevated/40 p-3 font-mono-data text-xs text-muted"
    >
      <div class="mb-1 grid grid-cols-4 gap-2">
        <span>significance</span>
        <span>effect size</span>
        <span>surprise</span>
        <span>kpi boost</span>
      </div>
      <div class="grid grid-cols-4 gap-2 text-highlighted">
        <span>{{ node.scoreBreakdown.significance.toFixed(3) }}</span>
        <span>{{ node.scoreBreakdown.effectSize.toFixed(3) }}</span>
        <span>{{ node.scoreBreakdown.surprise.toFixed(3) }}</span>
        <span>×{{ node.scoreBreakdown.kpiBoost.toFixed(2) }}</span>
      </div>
      <div v-if="node.filterChain.length > 0" class="mt-2 border-t border-default pt-2">
        Drill path: {{ node.filterChain.map((s) => `${s.column}=${s.value}`).join(' › ') }}
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
</template>
