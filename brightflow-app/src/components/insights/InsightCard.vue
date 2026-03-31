<script setup lang="ts">
import {
  AlertTriangle,
  ArrowLeftRight,
  BarChart3,
  ChevronDown,
  ChevronRight,
  GitBranch,
  Target,
  TrendingUp,
  Waves,
} from 'lucide-vue-next';
import { computed, ref } from 'vue';

import type { AnalysisNode, AnalysisTree } from '@/services/api';

const props = defineProps<{
  node: AnalysisNode;
  tree: AnalysisTree;
  depth?: number;
}>();

const expanded = ref(false);
const currentDepth = computed(() => props.depth ?? 0);

const childNodes = computed(() =>
  props.node.children
    .map((childId) => props.tree.nodes.find((n) => n.id['0'] === childId['0']))
    .filter((n): n is AnalysisNode => n !== undefined)
    .sort((a, b) => b.significance - a.significance),
);

const hasChildren = computed(() => childNodes.value.length > 0);

const analysisTypeConfig = computed(() => {
  const { type } = props.node.analysis;
  switch (type) {
    case 'Anomaly': {
      return { icon: AlertTriangle, color: 'text-red-500', bg: 'bg-red-500/10', label: 'Anomaly' };
    }
    case 'Trend': {
      return {
        icon: TrendingUp,
        color: 'text-blue-500',
        bg: 'bg-blue-500/10',
        label: 'Trend',
      };
    }
    case 'PeriodComparison': {
      return {
        icon: ArrowLeftRight,
        color: 'text-purple-500',
        bg: 'bg-purple-500/10',
        label: 'Period',
      };
    }
    case 'PeriodAnomaly': {
      return {
        icon: AlertTriangle,
        color: 'text-orange-500',
        bg: 'bg-orange-500/10',
        label: 'Period Anomaly',
      };
    }
    case 'Seasonality': {
      return {
        icon: Waves,
        color: 'text-teal-500',
        bg: 'bg-teal-500/10',
        label: 'Seasonality',
      };
    }
    case 'Segment': {
      return {
        icon: BarChart3,
        color: 'text-indigo-500',
        bg: 'bg-indigo-500/10',
        label: 'Segment',
      };
    }
    case 'Correlation': {
      return {
        icon: GitBranch,
        color: 'text-pink-500',
        bg: 'bg-pink-500/10',
        label: 'Correlation',
      };
    }
    case 'ForecastDeviation': {
      return {
        icon: Target,
        color: 'text-amber-500',
        bg: 'bg-amber-500/10',
        label: 'Forecast',
      };
    }
    case 'OutlierCluster': {
      return {
        icon: AlertTriangle,
        color: 'text-red-400',
        bg: 'bg-red-400/10',
        label: 'Outlier Cluster',
      };
    }
    default: {
      return {
        icon: BarChart3,
        color: 'text-gray-500',
        bg: 'bg-gray-500/10',
        label: type,
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
        <p class="font-mono-data mt-1 text-xs text-muted">
          {{ node.tech_summary }}
        </p>
      </div>

      <!-- Expand/collapse indicator -->
      <div v-if="hasChildren" class="mt-0.5 flex-shrink-0">
        <span class="mr-1 text-xs text-muted">{{ childNodes.length }}</span>
        <component :is="expanded ? ChevronDown : ChevronRight" class="inline h-4 w-4 text-muted" />
      </div>
    </button>

    <!-- Children (drill-down) -->
    <div v-if="expanded && hasChildren" class="space-y-2 px-4 pb-4">
      <InsightCard
        v-for="child in childNodes"
        :key="child.id['0']"
        :node="child"
        :tree="tree"
        :depth="currentDepth + 1"
      />
    </div>
  </div>
</template>
