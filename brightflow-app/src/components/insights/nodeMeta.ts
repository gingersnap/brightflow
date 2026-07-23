import {
  AlertTriangle,
  ArrowLeftRight,
  BarChart3,
  GitBranch,
  PieChart,
  Sigma,
  Split,
  Target,
  TrendingUp,
  Users,
  Waves,
} from '@lucide/vue';
import type { Component } from 'vue';

import type { AnalysisNode } from '@/services/api';

/**
 * Per-node metadata shared by the insights store, cards, and the optimistic
 * curation overlay.
 *
 * `measureOf` / `dimensionOf` are verbatim ports of the Rust `measure_of` /
 * `dimension_of` in `crates/brightflow-engine/src/analysis/select.rs` — the
 * server's suppress-by-measure/dimension filtering keys on those, so the
 * client overlay must match them exactly or optimistic hides diverge from the
 * next run. Keep the two in sync when either side changes.
 */

/** The measure a finding is about (matches Rust `select::measure_of`). */
export function measureOf(n: AnalysisNode): string {
  const a = n.analysis;
  switch (a.type) {
    case 'Anomaly':
    case 'Trend':
    case 'PeriodComparison':
    case 'PeriodAnomaly':
    case 'Seasonality':
    case 'ForecastDeviation':
    case 'Concentration':
    case 'DistributionShift':
    case 'ChangePoint': {
      return a.column;
    }
    case 'Segment': {
      return a.target_column;
    }
    case 'Correlation': {
      return a.column_a;
    }
    case 'RankChange':
    case 'TopDominance': {
      return a.measure;
    }
    case 'OutlierCluster': {
      return a.columns.join(',');
    }
    case 'MembershipChange': {
      return a.segment_column;
    }
  }
}

/** The dimension a finding slices on, if any (matches Rust `select::dimension_of`). */
export function dimensionOf(n: AnalysisNode): string | null {
  const a = n.analysis;
  if (a.type === 'Segment' || a.type === 'Concentration' || a.type === 'MembershipChange') {
    return a.segment_column;
  }
  if (a.type === 'RankChange' || a.type === 'TopDominance') {
    return a.dimension;
  }
  return n.filterChain[0]?.column ?? null;
}

/** Which way the finding points, for the direction filter and card indicator. */
export function directionOf(n: AnalysisNode): 'up' | 'down' | null {
  const a = n.analysis;
  switch (a.type) {
    case 'Anomaly': {
      return a.z_score > 0 ? 'up' : 'down';
    }
    case 'PeriodComparison':
    case 'PeriodAnomaly':
    case 'Segment': {
      return a.change_percent > 0 ? 'up' : 'down';
    }
    case 'Trend': {
      return a.direction === 'Increasing' ? 'up' : 'down';
    }
    case 'ForecastDeviation': {
      return a.deviation_percent > 0 ? 'up' : 'down';
    }
    case 'OutlierCluster': {
      return a.direction === 'spike' ? 'up' : 'down';
    }
    case 'ChangePoint': {
      return a.after_mean > a.before_mean ? 'up' : 'down';
    }
    case 'RankChange': {
      return a.new_rank < a.previous_rank ? 'up' : 'down';
    }
    case 'TopDominance':
    case 'Concentration':
    case 'Correlation':
    case 'DistributionShift':
    case 'MembershipChange':
    case 'Seasonality': {
      return null;
    }
  }
}

export interface AnalysisTypeMeta {
  bg: string;
  color: string;
  icon: Component;
  label: string;
}

/** Icon + color + human label per analysis type (cards, filter chips). */
export const ANALYSIS_TYPE_META: Record<AnalysisNode['analysis']['type'], AnalysisTypeMeta> = {
  Anomaly: { bg: 'bg-red-500/10', color: 'text-red-500', icon: AlertTriangle, label: 'Anomaly' },
  ChangePoint: { bg: 'bg-rose-500/10', color: 'text-rose-500', icon: Sigma, label: 'Change Point' },
  Concentration: {
    bg: 'bg-violet-500/10',
    color: 'text-violet-500',
    icon: PieChart,
    label: 'Concentration',
  },
  Correlation: {
    bg: 'bg-pink-500/10',
    color: 'text-pink-500',
    icon: GitBranch,
    label: 'Correlation',
  },
  DistributionShift: {
    bg: 'bg-cyan-500/10',
    color: 'text-cyan-500',
    icon: Split,
    label: 'Distribution Shift',
  },
  ForecastDeviation: {
    bg: 'bg-amber-500/10',
    color: 'text-amber-500',
    icon: Target,
    label: 'Forecast',
  },
  MembershipChange: {
    bg: 'bg-emerald-500/10',
    color: 'text-emerald-500',
    icon: Users,
    label: 'Membership',
  },
  OutlierCluster: {
    bg: 'bg-red-400/10',
    color: 'text-red-400',
    icon: AlertTriangle,
    label: 'Outlier Cluster',
  },
  PeriodAnomaly: {
    bg: 'bg-orange-500/10',
    color: 'text-orange-500',
    icon: AlertTriangle,
    label: 'Period Anomaly',
  },
  PeriodComparison: {
    bg: 'bg-purple-500/10',
    color: 'text-purple-500',
    icon: ArrowLeftRight,
    label: 'Period',
  },
  RankChange: {
    bg: 'bg-sky-500/10',
    color: 'text-sky-500',
    icon: ArrowLeftRight,
    label: 'Rank Change',
  },
  Seasonality: { bg: 'bg-teal-500/10', color: 'text-teal-500', icon: Waves, label: 'Seasonality' },
  Segment: { bg: 'bg-indigo-500/10', color: 'text-indigo-500', icon: BarChart3, label: 'Segment' },
  TopDominance: {
    bg: 'bg-fuchsia-500/10',
    color: 'text-fuchsia-500',
    icon: PieChart,
    label: 'Dominance',
  },
  Trend: { bg: 'bg-blue-500/10', color: 'text-blue-500', icon: TrendingUp, label: 'Trend' },
};

export interface QualitativeScore {
  label: 'Exceptional' | 'Strong' | 'Moderate' | 'Weak';
  /** Minimum composite score for this tier — also the filter-control stops. */
  threshold: number;
}

export const SCORE_TIERS: readonly QualitativeScore[] = [
  { label: 'Exceptional', threshold: 1.2 },
  { label: 'Strong', threshold: 0.8 },
  { label: 'Moderate', threshold: 0.5 },
  { label: 'Weak', threshold: 0 },
];

/** Composite score → qualitative tier for non-statistician-facing chrome. */
export function qualitativeScore(significance: number): QualitativeScore {
  return SCORE_TIERS.find((tier) => significance >= tier.threshold) ?? SCORE_TIERS.at(-1)!;
}
