/**
 * Shared ECharts option scaffold for the period-series renderers.
 *
 * Every period chart is "series over category labels with a compact value
 * axis": identical grid, tooltip, and axis chrome. The spec carries only
 * what differs per renderer — the series array, the labels, the y-axis name,
 * and a handful of knobs. Pure function, so the scaffold is testable without
 * echarts; the four non-period renderers (concentration, correlation,
 * segment, membership) have genuinely different shapes and stay hand-built.
 */

import { formatCompact, formatNumber } from '@/utils/format';

export interface PeriodSeriesSpec {
  labels: string[];
  series: Record<string, unknown>[];
  /** Y-axis name (already humanized by the caller). */
  yName: string;
  /** X-label formatter (e.g. humanizePeriodShort). */
  xFormatter?: (label: string) => string;
  /** X-label rotation in degrees. */
  rotate?: number;
  /** Grid bottom padding — default 24; larger when labels rotate or a legend sits below. */
  gridBottom?: number;
  /** Set false for line charts so the series touches both edges; omit for bars. */
  boundaryGap?: boolean;
  /** Show the standard bottom legend (series must carry `name`s). */
  legend?: boolean;
  /** Scale the y-axis to the data instead of starting at zero. */
  yScale?: boolean;
  /** Extra tooltip fields merged over the standard axis tooltip. */
  tooltip?: Record<string, unknown>;
  /** Field overrides merged over the built x/y axis (last wins). */
  xAxis?: Record<string, unknown>;
  yAxis?: Record<string, unknown>;
}

/** Build the full option object for one period-series chart. */
export function periodSeriesOption(spec: PeriodSeriesSpec): Record<string, unknown> {
  const axisLabel: Record<string, unknown> = { fontSize: 10 };
  if (spec.xFormatter) {
    axisLabel['formatter'] = spec.xFormatter;
  }
  if (spec.rotate !== undefined) {
    axisLabel['rotate'] = spec.rotate;
  }

  const option: Record<string, unknown> = {
    grid: { bottom: spec.gridBottom ?? 24, containLabel: true, left: 8, right: 8, top: 24 },
    series: spec.series,
    tooltip: {
      trigger: 'axis',
      valueFormatter: (v: number) => formatNumber(v),
      ...spec.tooltip,
    },
    xAxis: {
      axisLabel,
      ...(spec.boundaryGap === undefined ? {} : { boundaryGap: spec.boundaryGap }),
      data: spec.labels,
      type: 'category',
      ...spec.xAxis,
    },
    yAxis: {
      axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
      name: spec.yName,
      nameGap: 12,
      nameTextStyle: { fontSize: 10 },
      ...(spec.yScale === true ? { scale: true } : {}),
      type: 'value',
      ...spec.yAxis,
    },
  };
  if (spec.legend === true) {
    option['legend'] = { bottom: 0, itemHeight: 6, itemWidth: 12, textStyle: { fontSize: 10 } };
  }
  return option;
}
