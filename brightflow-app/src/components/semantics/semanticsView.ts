/**
 * Pure helpers for the Semantics page: the resolved column reshaped for the
 * column menu, an opinion row's stated fields as label/value pairs, and the
 * order the page lists opinions in — highest layer first, the order the
 * store resolves them in, newest first within a layer. Nothing here fetches
 * or dispatches.
 */

import type {
  ColumnInfo,
  ColumnOpinion,
  DocFields,
  Layer,
  ResolvedColumn,
  TableOpinion,
} from '@/types/generated';
import { POLARITY_LABELS, ROLE_LABELS } from '@/utils/semanticLabels';

/** Layers from the one that wins down to the one every other outranks. */
export const LAYER_ORDER: Layer[] = ['user', 'agent', 'declared', 'detected'];

export const LAYER_LABELS: Record<Layer, string> = {
  agent: 'Agent',
  declared: 'Declared',
  detected: 'Detected',
  user: 'Person',
};

/** One field a layer stated, as the page shows it. */
export interface StatedField {
  field: string;
  value: string;
}

/** A blank string is a layer clearing the field, which is a statement too. */
function text(value: string): string {
  return value === '' ? '(cleared)' : value;
}

/**
 * The column as the menu and edit composable expect it. `ColumnInfo`
 * carries a datatype that the resolved view may lack; the menu never reads
 * it, so a missing one is filled with `String`.
 */
export function toColumnInfo(column: ResolvedColumn): ColumnInfo {
  return {
    datatype: column.datatype ?? 'String',
    isKpi: column.is_kpi ?? null,
    label: column.label ?? null,
    name: column.name,
    role: column.role ?? null,
    ...(column.polarity == null ? {} : { polarity: column.polarity }),
    ...(column.description == null ? {} : { description: column.description }),
    ...(column.resolved_by == null ? {} : { resolvedBy: column.resolved_by }),
  };
}

/** The fields one layer stated about a column, in the page's column order. */
export function columnOpinionFields(opinion: ColumnOpinion): StatedField[] {
  const fields: StatedField[] = [];
  if (opinion.datatype != null) {
    fields.push({ field: 'type', value: opinion.datatype });
  }
  if (opinion.is_time != null) {
    fields.push({ field: 'time axis', value: opinion.is_time ? 'yes' : 'no' });
  }
  if (opinion.ext.role != null) {
    fields.push({ field: 'role', value: ROLE_LABELS[opinion.ext.role] });
  }
  if (opinion.ext.is_kpi != null) {
    fields.push({ field: 'KPI', value: opinion.ext.is_kpi ? 'yes' : 'no' });
  }
  if (opinion.ext.polarity != null) {
    fields.push({ field: 'polarity', value: POLARITY_LABELS[opinion.ext.polarity] });
  }
  if (opinion.ext.label != null) {
    fields.push({ field: 'label', value: text(opinion.ext.label) });
  }
  if (opinion.description != null) {
    fields.push({ field: 'description', value: text(opinion.description) });
  }
  return fields;
}

/** "id: id · title: title · text: body · time: created_at · link: {html_url}". */
export function docSummary(doc: DocFields): string {
  const parts: string[] = [];
  if (doc.id != null) {
    parts.push(`id: ${doc.id}`);
  }
  if (doc.number != null) {
    parts.push(`number: ${doc.number}`);
  }
  if (doc.title != null) {
    parts.push(`title: ${doc.title}`);
  }
  if (doc.body != null) {
    parts.push(`text: ${doc.body}`);
  }
  if (doc.timestamp != null) {
    parts.push(`time: ${doc.timestamp}`);
  }
  if (doc.url_template != null) {
    parts.push(`link: ${doc.url_template}`);
  }
  return parts.join(' · ');
}

/** The fields one layer stated about the table itself. */
export function tableOpinionFields(opinion: TableOpinion): StatedField[] {
  const fields: StatedField[] = [];
  if (opinion.display_name != null) {
    fields.push({ field: 'display name', value: text(opinion.display_name) });
  }
  if (opinion.description != null) {
    fields.push({ field: 'description', value: text(opinion.description) });
  }
  if (opinion.time_granularity != null) {
    fields.push({ field: 'analysis period', value: opinion.time_granularity });
  }
  if (opinion.comparison_periods != null) {
    fields.push({ field: 'periods compared', value: String(opinion.comparison_periods) });
  }
  if (opinion.doc != null) {
    fields.push({ field: 'doc columns', value: docSummary(opinion.doc) });
  }
  return fields;
}

/** Opinions highest layer first, newest first within a layer; a new array. */
export function sortOpinions<
  T extends { provenance: { layer: Layer }; updated_at: bigint | number },
>(opinions: T[]): T[] {
  return opinions.toSorted((a, b) => {
    const byLayer =
      LAYER_ORDER.indexOf(a.provenance.layer) - LAYER_ORDER.indexOf(b.provenance.layer);
    return byLayer === 0 ? Number(b.updated_at) - Number(a.updated_at) : byLayer;
  });
}

/** The opinion rows behind one column, in display order. */
export function opinionsFor(layers: ColumnOpinion[], column: string): ColumnOpinion[] {
  return sortOpinions(layers.filter((o) => o.column === column));
}

/** "connector:github 0.3.0" — the producer with its version when it has one. */
export function producerText(provenance: { producer: string; version?: string }): string {
  return provenance.version == null
    ? provenance.producer
    : `${provenance.producer} ${provenance.version}`;
}
