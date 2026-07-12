import type { Component } from 'vue';

import AnomalyRenderer from './AnomalyRenderer.vue';
import ConcentrationRenderer from './ConcentrationRenderer.vue';
import CorrelationRenderer from './CorrelationRenderer.vue';
import DistributionShiftRenderer from './DistributionShiftRenderer.vue';
import ForecastRenderer from './ForecastRenderer.vue';
import MembershipRenderer from './MembershipRenderer.vue';
import OutlierClusterRenderer from './OutlierClusterRenderer.vue';
import PeriodAnomalyRenderer from './PeriodAnomalyRenderer.vue';
import PeriodComparisonRenderer from './PeriodComparisonRenderer.vue';
import SeasonalityRenderer from './SeasonalityRenderer.vue';
import SegmentRenderer from './SegmentRenderer.vue';
import TrendRenderer from './TrendRenderer.vue';

const RENDERERS: Record<string, Component> = {
  Anomaly: AnomalyRenderer,
  // ChangePoint produces SeriesWithFit data, reuse the trend renderer
  ChangePoint: TrendRenderer,
  Concentration: ConcentrationRenderer,
  Correlation: CorrelationRenderer,
  DistributionShift: DistributionShiftRenderer,
  ForecastDeviation: ForecastRenderer,
  MembershipChange: MembershipRenderer,
  OutlierCluster: OutlierClusterRenderer,
  PeriodAnomaly: PeriodAnomalyRenderer,
  PeriodComparison: PeriodComparisonRenderer,
  // RankChange carries a marked Series (rank per period) — reuse the
  // Period-anomaly line renderer; v2 gets a dedicated bump chart.
  RankChange: PeriodAnomalyRenderer,
  // TopDominance carries SegmentBars — reuse the segment bar renderer.
  TopDominance: SegmentRenderer,
  Seasonality: SeasonalityRenderer,
  Segment: SegmentRenderer,
  Trend: TrendRenderer,
};

export function rendererFor(type: string): Component | null {
  return RENDERERS[type] ?? null;
}
