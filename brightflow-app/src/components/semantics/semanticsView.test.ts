/**
 * Unit tests for the Semantics page's pure helpers: the menu shape, the
 * stated fields of an opinion, layer ordering and producer text.
 */

import { describe, expect, test } from 'vitest';

import type { ColumnOpinion, TableOpinion } from '@/types/generated';

import {
  columnOpinionFields,
  docSummary,
  opinionsFor,
  producerText,
  sortOpinions,
  tableOpinionFields,
  toColumnInfo,
} from './semanticsView';

function opinion(
  column: string,
  layer: ColumnOpinion['provenance']['layer'],
  extra: Partial<ColumnOpinion> = {},
): ColumnOpinion {
  return {
    column,
    custom_extensions: [],
    ext: {},
    provenance: { layer, producer: `${layer}:x` },
    updated_at: 0n,
    ...extra,
  };
}

describe('toColumnInfo', () => {
  test('maps the resolved column onto the menu shape without inventing optionals', () => {
    const info = toColumnInfo({ name: 'revenue', role: 'measure', is_kpi: true });
    expect(info).toEqual({
      datatype: 'String',
      isKpi: true,
      label: null,
      name: 'revenue',
      role: 'measure',
    });
    expect('polarity' in info).toBe(false);
    const full = toColumnInfo({
      name: 'r',
      datatype: 'Float',
      polarity: 'higher_is_better',
      description: 'Net revenue',
      resolved_by: { layer: 'user', producer: 'user:1' },
    });
    expect(full.datatype).toBe('Float');
    expect(full.polarity).toBe('higher_is_better');
    expect(full.resolvedBy?.layer).toBe('user');
  });
});

describe('columnOpinionFields', () => {
  test('lists only what the layer stated and marks a blank as cleared', () => {
    const fields = columnOpinionFields(
      opinion('r', 'user', {
        description: '',
        ext: { role: 'measure', label: 'Revenue', is_kpi: false },
      }),
    );
    expect(fields).toEqual([
      { field: 'role', value: 'Measure' },
      { field: 'KPI', value: 'no' },
      { field: 'label', value: 'Revenue' },
      { field: 'description', value: '(cleared)' },
    ]);
    expect(columnOpinionFields(opinion('r', 'detected'))).toEqual([]);
  });
});

describe('tableOpinionFields and docSummary', () => {
  test('render the table statement including the doc columns', () => {
    const table: TableOpinion = {
      custom_extensions: [],
      provenance: { layer: 'detected', producer: 'detector' },
      updated_at: 0n,
      description: 'One row per issue',
      time_granularity: 'week',
      doc: { id: 'id', title: 'title', body: 'body', url_template: '{html_url}' },
    };
    expect(tableOpinionFields(table)).toEqual([
      { field: 'description', value: 'One row per issue' },
      { field: 'analysis period', value: 'week' },
      { field: 'doc columns', value: 'id: id · title: title · text: body · link: {html_url}' },
    ]);
    expect(docSummary({})).toBe('');
  });
});

describe('sortOpinions and opinionsFor', () => {
  test('orders the winning layer first and newest first within a layer', () => {
    const rows = [
      opinion('r', 'detected', { updated_at: 5n }),
      opinion('r', 'user', { updated_at: 1n }),
      opinion('r', 'agent', { updated_at: 2n }),
      opinion('r', 'agent', { updated_at: 9n }),
      opinion('other', 'user', { updated_at: 7n }),
    ];
    const sorted = sortOpinions(rows).map((o) => `${o.provenance.layer}:${o.updated_at}`);
    expect(sorted).toEqual(['user:7', 'user:1', 'agent:9', 'agent:2', 'detected:5']);
    // The input is not reordered in place.
    expect(rows[0]?.provenance.layer).toBe('detected');
    expect(opinionsFor(rows, 'other')).toHaveLength(1);
  });
});

describe('producerText', () => {
  test('appends the version only when there is one', () => {
    expect(producerText({ producer: 'connector:github', version: '0.3.0' })).toBe(
      'connector:github 0.3.0',
    );
    expect(producerText({ producer: 'detector' })).toBe('detector');
  });
});
