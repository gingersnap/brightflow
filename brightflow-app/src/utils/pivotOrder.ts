/**
 * Display order for pivot results, shared by the pivot table and the chart
 * so the two views always agree.
 *
 * Sort is a property of a field, the way Excel treats it: each row field
 * and the column field carries a `FieldSort`, by label or by value, in either
 * direction. Row fields nest, so the first field orders the groups and each
 * later field orders the rows within its parent group; "value" for a group
 * is the sum of every value column over the group's rows. The backend
 * returns aggregates in whatever order Polars produced, and nothing here
 * assumes one.
 *
 * Ties break the same way everywhere: `other` after everything else, null
 * labels last, then the label text. A value sort therefore keeps `other`
 * wherever its size puts it — the point of showing it — and only pushes it
 * back among equals.
 */

export interface FieldSort {
  by: 'label' | 'value';
  descending: boolean;
}

/** What a new row or column field starts with: largest first. */
export const DEFAULT_FIELD_SORT: FieldSort = { by: 'value', descending: true };

export const FIELD_SORT_OPTIONS: { label: string; value: FieldSort }[] = [
  { label: 'Largest first', value: { by: 'value', descending: true } },
  { label: 'Smallest first', value: { by: 'value', descending: false } },
  { label: 'A → Z', value: { by: 'label', descending: false } },
  { label: 'Z → A', value: { by: 'label', descending: true } },
];

export function sortKey(sort: FieldSort): string {
  return `${sort.by}:${sort.descending ? 'desc' : 'asc'}`;
}

function isOther(label: unknown): boolean {
  return typeof label === 'string' && label.trim().toLowerCase() === 'other';
}

/** Tie-break shared by every sort: null last, `other` just before it, then text. */
function compareLabels(a: unknown, b: unknown): number {
  if ((a == null) !== (b == null)) {
    return a == null ? 1 : -1;
  }
  if (isOther(a) !== isOther(b)) {
    return isOther(a) ? 1 : -1;
  }
  return labelText(a).localeCompare(labelText(b));
}

/** Labels are strings or numbers in practice; anything else compares by its JSON. */
function labelText(v: unknown): string {
  if (typeof v === 'string') {
    return v;
  }
  if (typeof v === 'number' || typeof v === 'bigint' || typeof v === 'boolean') {
    return String(v);
  }
  return v == null ? '' : (JSON.stringify(v) ?? '');
}

function compare(
  a: { label: unknown; total: number },
  b: { label: unknown; total: number },
  sort: FieldSort,
): number {
  if (sort.by === 'value') {
    const diff = sort.descending ? b.total - a.total : a.total - b.total;
    if (diff !== 0) {
      return diff;
    }
    return compareLabels(a.label, b.label);
  }
  // Label sorts flip the text comparison but keep `other` and null at the
  // End either way — they are not part of the alphabet.
  const otherOrNull = (v: unknown): boolean => isOther(v) || v == null;
  if (otherOrNull(a.label) || otherOrNull(b.label)) {
    if (otherOrNull(a.label) !== otherOrNull(b.label)) {
      return otherOrNull(a.label) ? 1 : -1;
    }
    return compareLabels(a.label, b.label);
  }
  const text = compareLabels(a.label, b.label);
  return sort.descending ? -text : text;
}

function numeric(v: unknown): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : 0;
}

function rowTotal(row: unknown[], valueIdx: number[]): number {
  let total = 0;
  for (const i of valueIdx) {
    total += numeric(row[i]);
  }
  return total;
}

export interface RowOrderInput {
  rows: unknown[][];
  /** Row-field columns in nesting order. */
  indexIdx: number[];
  /** Value columns; a group's "value" is their sum over its rows. */
  valueIdx: number[];
  /** One per row field; missing entries fall back to the default. */
  sorts: FieldSort[];
}

/** Row indices in display order. */
export function orderRows({ rows, indexIdx, valueIdx, sorts }: RowOrderInput): number[] {
  const order = (candidates: number[], level: number): number[] => {
    if (level >= indexIdx.length || candidates.length === 0) {
      return candidates;
    }
    const col = indexIdx[level] ?? 0;
    const sort = sorts[level] ?? DEFAULT_FIELD_SORT;
    const groups = new Map<unknown, { label: unknown; total: number; rows: number[] }>();
    for (const r of candidates) {
      const label = rows[r]?.[col];
      let group = groups.get(label);
      if (group == null) {
        group = { label, total: 0, rows: [] };
        groups.set(label, group);
      }
      group.rows.push(r);
      group.total += rowTotal(rows[r] ?? [], valueIdx);
    }
    const sorted = [...groups.values()].toSorted((a, b) => compare(a, b, sort));
    return sorted.flatMap((g) => order(g.rows, level + 1));
  };
  return order(
    rows.map((_, i) => i),
    0,
  );
}

export interface SeriesOrderInput {
  rows: unknown[][];
  /** Value columns, one per series. */
  seriesIdx: number[];
  /** Column names, indexed like the rows. */
  names: string[];
  /** The column field's sort: total of the column, or its name. */
  sort: FieldSort;
}

/** Series (value column) indices in display order. */
export function orderSeries({ rows, seriesIdx, names, sort }: SeriesOrderInput): number[] {
  const entries = seriesIdx.map((i) => {
    let total = 0;
    for (const row of rows) {
      total += numeric(row[i]);
    }
    return { idx: i, label: names[i], total };
  });
  return entries.toSorted((a, b) => compare(a, b, sort)).map((e) => e.idx);
}
