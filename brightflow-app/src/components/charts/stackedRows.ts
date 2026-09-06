/**
 * Stacking the inner row field inside the outer field's bar — the chart of
 * a nested pivot table (Rows = category then subcategory, no column field),
 * which Excel cannot draw.
 *
 * The long group-by result has one row per (outer, inner) pair. Bars are
 * the distinct outer values; each bar's segments are its inner values. The
 * series are *by position*, not by inner name: series k holds the k-th
 * segment of every bar, so ten bars with at most ten children make ten
 * series rather than seventy-five, and every data point carries its own
 * inner name for hover and labels. Both orders come in from the caller
 * (see `utils/pivotOrder`): bars in the order rows first appear, segments
 * in row order within the bar.
 */

export interface Segment {
  name: string;
  value: number;
  /** Share of its bar's total; 0 when the bar is empty. */
  share: number;
}

export interface StackedRows {
  /** Bar labels, in display order. */
  bars: string[];
  /** Per bar, its segments in stack order. */
  segments: Segment[][];
  /** Series by position: `data[i]` is bar i's rank-th segment, or null. */
  series: { rank: number; data: (Segment | null)[] }[];
}

export interface StackedRowsInput {
  rows: unknown[][];
  outerIdx: number;
  innerIdx: number;
  valueIdx: number;
  /** Row indices in display order (nested sort already applied). */
  order: number[];
}

function label(v: unknown): string {
  if (v == null) {
    return 'null';
  }
  return typeof v === 'string' ? v : JSON.stringify(v);
}

function numeric(v: unknown): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : 0;
}

export function stackedRowSeries({
  rows,
  outerIdx,
  innerIdx,
  valueIdx,
  order,
}: StackedRowsInput): StackedRows {
  const bars: string[] = [];
  const segments: Segment[][] = [];
  const barIndex = new Map<string, number>();
  for (const row of order.map((r) => rows[r]).filter((r) => r != null)) {
    const bar = label(row[outerIdx]);
    let i = barIndex.get(bar);
    if (i == null) {
      i = bars.length;
      barIndex.set(bar, i);
      bars.push(bar);
      segments.push([]);
    }
    segments[i]?.push({ name: label(row[innerIdx]), value: numeric(row[valueIdx]), share: 0 });
  }
  for (const list of segments) {
    const total = list.reduce((sum, s) => sum + s.value, 0);
    for (const s of list) {
      s.share = total === 0 ? 0 : s.value / total;
    }
  }
  const depth = Math.max(0, ...segments.map((s) => s.length));
  const series = Array.from({ length: depth }, (_, rank) => ({
    rank,
    data: segments.map((list) => list[rank] ?? null),
  }));
  return { bars, segments, series };
}
