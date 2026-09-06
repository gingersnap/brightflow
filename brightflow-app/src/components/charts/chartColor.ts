/**
 * Colour rules for stacked series, kept pure so they are testable.
 *
 * Two rules, both about categorical hues never cycling:
 *
 * - `seriesConfinedToOneBar`: when every series has values in exactly one
 *   bar (a parent/child pair built wide — category rows, subcategory
 *   columns), colour should follow the *bar*, not the series: one hue per
 *   bar, its series as descending opacity of it. The axis label carries the
 *   bar's identity, so hue is decoration there and may repeat past the
 *   palette; series identity comes from the legend and hover, not colour.
 * - `foldSeries`: when series are not confined and outnumber the palette,
 *   the smallest fold into one "Other" series rather than reusing hues.
 *
 * `other` and null series take the muted colour at every level.
 */

export function isOtherName(name: unknown): boolean {
  return name == null || (typeof name === 'string' && name.trim().toLowerCase() === 'other');
}

function numeric(v: unknown): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : 0;
}

/**
 * For every series, the single row it has non-zero values in; null when
 * any series spans two rows. Series with no values at all are confined
 * trivially and map to -1. `other` and null series are exempt from the
 * test, since they legitimately appear under every parent.
 */
export function seriesConfinedToOneBar(
  rows: unknown[][],
  seriesIdx: number[],
  names: string[],
): Map<number, number> | null {
  const home = new Map<number, number>();
  for (const s of seriesIdx) {
    const present = rows.map((row, r) => (numeric(row[s]) === 0 ? -1 : r)).filter((r) => r !== -1);
    if (present.length > 1 && !isOtherName(names[s])) {
      return null;
    }
    home.set(s, isOtherName(names[s]) ? -1 : (present[0] ?? -1));
  }
  return home;
}

/** Opacity for the rank-th of `count` segments: most opaque first, floor 0.35. */
export function opacityForRank(rank: number, count: number): number {
  if (count <= 1) {
    return 1;
  }
  const floor = 0.35;
  return 1 - (rank / (count - 1)) * (1 - floor);
}

/**
 * Ranks each confined series within its bar by value, largest first, so
 * `opacityForRank` can shade it. Returns `{ rank, count }` per series;
 * `other` series get the last rank of the bar they sit in.
 */
export function rankWithinBars(
  rows: unknown[][],
  seriesIdx: number[],
  home: Map<number, number>,
): Map<number, { rank: number; count: number }> {
  const byBar = new Map<number, number[]>();
  for (const s of seriesIdx) {
    const bar = home.get(s) ?? -1;
    if (bar !== -1) {
      const list = byBar.get(bar) ?? [];
      list.push(s);
      byBar.set(bar, list);
    }
  }
  const out = new Map<number, { rank: number; count: number }>();
  for (const [bar, list] of byBar) {
    const row = rows[bar] ?? [];
    const sorted = [...list].toSorted((a, b) => numeric(row[b]) - numeric(row[a]));
    for (const [rank, s] of sorted.entries()) {
      out.set(s, { rank, count: sorted.length });
    }
  }
  return out;
}

export interface Fold {
  /** Series kept as they are, in the order given. */
  kept: number[];
  /** Series summed into the synthetic "Other" series; empty when none. */
  folded: number[];
}

/**
 * Keep the first `keep` series (already in display order, so the largest)
 * and fold the rest. Series already named `other` fold too, so the chart
 * ends with one `other` at most.
 */
export function foldSeries(seriesIdx: number[], names: string[], keep: number): Fold {
  const kept: number[] = [];
  const folded: number[] = [];
  for (const s of seriesIdx) {
    if (kept.length < keep && !isOtherName(names[s])) {
      kept.push(s);
    } else {
      folded.push(s);
    }
  }
  if (folded.length === 1 && isOtherName(names[folded[0] ?? -1])) {
    // A lone `other` needs no folding; keep it as the real series.
    return { kept: [...kept, ...folded], folded: [] };
  }
  return { kept, folded };
}

/** The folded series' values per row: the sum of what was folded. */
export function foldedColumn(rows: unknown[][], folded: number[]): number[] {
  return rows.map((row) => folded.reduce((sum, s) => sum + numeric(row[s]), 0));
}
