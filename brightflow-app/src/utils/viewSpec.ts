/**
 * What a saved Explore view stores and how it is put back: a snapshot of the
 * query store (filters, sort, limit, section toggles) and the pivot store
 * (buckets and display settings). The spec is versioned so a later builder
 * can read an older one; the server stores it opaquely. `parse` accepts only
 * a shape this version wrote, so a foreign or damaged spec applies nothing.
 */

import { toRaw } from 'vue';

import type { Filter, PivotField, QuerySections, SectionState } from '@/types';

export const VIEW_SPEC_VERSION = 1;

export interface ExploreViewSpec {
  version: typeof VIEW_SPEC_VERSION;
  query: {
    filters: Filter[];
    sortBy: string | null;
    sortDescending: boolean;
    limit: number;
    sections: QuerySections;
  };
  pivot: {
    rowFields: PivotField[];
    columnFields: PivotField[];
    valueFields: PivotField[];
    showSubtotals: boolean;
    showColumnTotals: boolean;
    showConditionalFormatting: boolean;
    decimalPlaces: number;
  };
}

/** The slice of the query store a view captures. */
export interface QuerySnapshot {
  filters: Filter[];
  sortBy: string | null;
  sortDescending: boolean;
  limit: number;
  sections: QuerySections;
}

/** The slice of the pivot store a view captures. */
export interface PivotSnapshot {
  rowFields: PivotField[];
  columnFields: PivotField[];
  valueFields: PivotField[];
  showSubtotals: boolean;
  showColumnTotals: boolean;
  showConditionalFormatting: boolean;
  decimalPlaces: number;
}

/**
 * Deep-copy so the spec never shares objects with the stores. The inputs come
 * straight out of Pinia as Vue reactive proxies, which `structuredClone`
 * refuses, so every level is unwrapped with `toRaw` first.
 */
function unwrap(value: unknown): unknown {
  const raw: unknown = toRaw(value);
  if (Array.isArray(raw)) {
    return raw.map((item) => unwrap(item));
  }
  if (typeof raw === 'object' && raw !== null) {
    return Object.fromEntries(Object.entries(raw).map(([key, item]) => [key, unwrap(item)]));
  }
  return raw;
}

function clone<T>(value: T): T {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- unwrap keeps the shape, only the proxies go
  return structuredClone(unwrap(value) as T);
}

export function captureExploreView(query: QuerySnapshot, pivot: PivotSnapshot): ExploreViewSpec {
  return clone({
    pivot: {
      columnFields: pivot.columnFields,
      decimalPlaces: pivot.decimalPlaces,
      rowFields: pivot.rowFields,
      showColumnTotals: pivot.showColumnTotals,
      showConditionalFormatting: pivot.showConditionalFormatting,
      showSubtotals: pivot.showSubtotals,
      valueFields: pivot.valueFields,
    },
    query: {
      filters: query.filters,
      limit: query.limit,
      sections: query.sections,
      sortBy: query.sortBy,
      sortDescending: query.sortDescending,
    },
    version: VIEW_SPEC_VERSION,
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value != null && !Array.isArray(value);
}

function isSection(value: unknown): value is SectionState {
  return (
    isRecord(value) &&
    typeof value['enabled'] === 'boolean' &&
    typeof value['collapsed'] === 'boolean'
  );
}

function isSections(value: unknown): value is QuerySections {
  return (
    isRecord(value) &&
    isSection(value['filter']) &&
    isSection(value['sort']) &&
    isSection(value['limit'])
  );
}

function isFilter(value: unknown): value is Filter {
  return (
    isRecord(value) &&
    typeof value['id'] === 'string' &&
    (typeof value['column'] === 'string' || value['column'] === null) &&
    typeof value['op'] === 'string' &&
    'value' in value
  );
}

function isPivotField(value: unknown): value is PivotField {
  return (
    isRecord(value) &&
    typeof value['id'] === 'string' &&
    typeof value['column'] === 'string' &&
    typeof value['datatype'] === 'string'
  );
}

function every<T>(value: unknown, guard: (item: unknown) => item is T): value is T[] {
  return Array.isArray(value) && value.every((item) => guard(item));
}

/**
 * A spec this version wrote, rebuilt field by field, or null when any part
 * is not the shape the builder writes. Nothing is trusted on the way in.
 */
export function parseExploreView(spec: unknown): ExploreViewSpec | null {
  if (!isRecord(spec) || spec['version'] !== VIEW_SPEC_VERSION) {
    return null;
  }
  const query = spec['query'];
  const pivot = spec['pivot'];
  if (!isRecord(query) || !isRecord(pivot)) {
    return null;
  }
  const filters = query['filters'];
  const sections = query['sections'];
  const sortBy = query['sortBy'];
  const rowFields = pivot['rowFields'];
  const columnFields = pivot['columnFields'];
  const valueFields = pivot['valueFields'];
  if (
    !every(filters, isFilter) ||
    !isSections(sections) ||
    !(typeof sortBy === 'string' || sortBy === null) ||
    typeof query['sortDescending'] !== 'boolean' ||
    typeof query['limit'] !== 'number' ||
    !every(rowFields, isPivotField) ||
    !every(columnFields, isPivotField) ||
    !every(valueFields, isPivotField) ||
    typeof pivot['showSubtotals'] !== 'boolean' ||
    typeof pivot['showColumnTotals'] !== 'boolean' ||
    typeof pivot['showConditionalFormatting'] !== 'boolean' ||
    typeof pivot['decimalPlaces'] !== 'number'
  ) {
    return null;
  }
  return {
    pivot: {
      columnFields,
      decimalPlaces: pivot['decimalPlaces'],
      rowFields,
      showColumnTotals: pivot['showColumnTotals'],
      showConditionalFormatting: pivot['showConditionalFormatting'],
      showSubtotals: pivot['showSubtotals'],
      valueFields,
    },
    query: {
      filters,
      limit: query['limit'],
      sections,
      sortBy,
      sortDescending: query['sortDescending'],
    },
    version: VIEW_SPEC_VERSION,
  };
}

/**
 * Write a spec into the stores. The stores' own watchers re-run the table
 * and pivot queries, so nothing here executes anything. Copies again on the
 * way in so a second apply of the same view starts from the saved state.
 */
export function applyExploreView(
  spec: ExploreViewSpec,
  query: QuerySnapshot,
  pivot: PivotSnapshot,
): void {
  const fresh = clone(spec);
  query.filters = fresh.query.filters;
  query.sortBy = fresh.query.sortBy;
  query.sortDescending = fresh.query.sortDescending;
  query.limit = fresh.query.limit;
  query.sections = fresh.query.sections;
  pivot.rowFields = fresh.pivot.rowFields;
  pivot.columnFields = fresh.pivot.columnFields;
  pivot.valueFields = fresh.pivot.valueFields;
  pivot.showSubtotals = fresh.pivot.showSubtotals;
  pivot.showColumnTotals = fresh.pivot.showColumnTotals;
  pivot.showConditionalFormatting = fresh.pivot.showConditionalFormatting;
  pivot.decimalPlaces = fresh.pivot.decimalPlaces;
}

/** "3 filters · 2 values · by region" — what a view does, for its row. */
export function describeExploreView(spec: ExploreViewSpec): string {
  const parts: string[] = [];
  const filters = spec.query.sections.filter.enabled ? spec.query.filters.length : 0;
  if (filters > 0) {
    parts.push(`${filters} filter${filters === 1 ? '' : 's'}`);
  }
  const values = spec.pivot.valueFields.length;
  if (values > 0) {
    parts.push(`${values} value${values === 1 ? '' : 's'}`);
    const by = [...spec.pivot.rowFields, ...spec.pivot.columnFields].map((f) => f.column);
    if (by.length > 0) {
      parts.push(`by ${by.join(', ')}`);
    }
  }
  if (spec.query.sections.sort.enabled && spec.query.sortBy != null) {
    parts.push(`sorted by ${spec.query.sortBy}`);
  }
  return parts.length > 0 ? parts.join(' · ') : 'plain table';
}
