/**
 * Unit tests for the dataset store's semantic surface: which columns Explore
 * offers, how a column is labelled, which columns count as time, and how an
 * applied action event patches a column in place. The load/reset plumbing
 * and the UI-store reset it triggers are out of scope.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test, vi } from 'vitest';

import type { ColumnInfo, LoadTableResponse } from '@/types';
import type { ActionLogEntry, LogicalType } from '@/types/generated';

import { isTimeColumn, useDatasetStore } from './dataset';

// The UI store touches `document` on setup; this store only calls its reset.
vi.mock('./ui', () => ({ useUiStore: () => ({ resetForNewDataset: (): void => {} }) }));

function col(name: string, datatype: LogicalType, extra: Partial<ColumnInfo> = {}): ColumnInfo {
  return { datatype, isKpi: null, label: null, name, role: null, ...extra };
}

function load(columns: ColumnInfo[], timeGranularity?: LoadTableResponse['timeGranularity']) {
  const store = useDatasetStore();
  const response: LoadTableResponse = {
    columnCount: columns.length,
    columns,
    id: 'store:s|issues',
    name: 'issues',
    rowCount: 3,
  };
  if (timeGranularity != null) {
    response.timeGranularity = timeGranularity;
  }
  store.setFromLoadResponse(response);
  return store;
}

function entry(params: Record<string, unknown>, status = 'applied'): ActionLogEntry {
  return {
    actionKind: String(params['kind']),
    actorType: 'human',
    createdAt: 0,
    id: 1,
    params,
    requestId: 'r',
    status,
    undoable: true,
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('visibleColumns and labels', () => {
  test('ignored columns are hidden; labels fall back to the humanised name', () => {
    const store = load([
      col('order_total', 'Float', { label: 'Revenue', role: 'measure' }),
      col('summary', 'String', { role: 'ignored' }),
      col('created_at', 'String', { role: 'time' }),
    ]);
    expect(store.visibleColumns.map((c) => c.name)).toEqual(['order_total', 'created_at']);
    expect(store.labelFor('order_total')).toBe('Revenue');
    expect(store.labelFor('created_at')).toBe('Created At');
    expect(store.labelFor('not_a_column')).toBe('Not A Column');
  });

  test('time columns come from role first, logical type second', () => {
    const store = load([
      col('created_at', 'String', { role: 'time' }),
      col('ts', 'DateTime'),
      col('ts_as_dim', 'DateTime', { role: 'dimension' }),
      col('note', 'String'),
    ]);
    expect(store.timeColumns.map((c) => c.name)).toEqual(['created_at', 'ts']);
    expect(isTimeColumn({ datatype: 'Date', role: null })).toBe(true);
    expect(isTimeColumn({ datatype: 'Date', role: 'dimension' })).toBe(false);
  });

  test('the table granularity defaults to week', () => {
    expect(load([col('a', 'String')]).timeGranularity).toBe('week');
    expect(load([col('a', 'String')], 'month').timeGranularity).toBe('month');
  });
});

describe('applySemanticAction', () => {
  test('patches role, label, description, KPI and polarity on the loaded table', () => {
    const store = load([col('units', 'Integer', { isKpi: true, role: 'measure' })]);
    const base = { column: 'units', source_id: 's', table: 'issues' };

    expect(
      store.applySemanticAction(
        entry({ ...base, kind: 'set_column_label', label: ' Units sold ' }),
      ),
    ).toBe(true);
    expect(store.columnByName('units')?.label).toBe('Units sold');

    expect(
      store.applySemanticAction(
        entry({ ...base, kind: 'set_column_description', description: 'Count of items' }),
      ),
    ).toBe(true);
    expect(store.columnByName('units')?.description).toBe('Count of items');

    expect(
      store.applySemanticAction(
        entry({ ...base, kind: 'set_column_polarity', polarity: 'higher_is_better' }),
      ),
    ).toBe(true);
    expect(store.columnByName('units')?.polarity).toBe('higher_is_better');

    // A role change off measure clears the KPI flag, as the backend does.
    expect(
      store.applySemanticAction(entry({ ...base, kind: 'set_column_role', role: 'dimension' })),
    ).toBe(true);
    expect(store.columnByName('units')?.role).toBe('dimension');
    expect(store.columnByName('units')?.isKpi).toBe(false);

    expect(store.applySemanticAction(entry({ ...base, kind: 'set_kpi', is_kpi: true }))).toBe(true);
    expect(store.columnByName('units')?.isKpi).toBe(true);

    // A blank label clears.
    expect(
      store.applySemanticAction(entry({ ...base, kind: 'set_column_label', label: '  ' })),
    ).toBe(true);
    expect(store.columnByName('units')?.label).toBeNull();
  });

  test('ignores other tables, other kinds, unknown columns and non-applied entries', () => {
    const store = load([col('units', 'Integer')]);
    const base = { column: 'units', kind: 'set_column_label', label: 'X', source_id: 's' };
    expect(store.applySemanticAction(entry({ ...base, table: 'orders' }))).toBe(false);
    expect(store.applySemanticAction(entry({ ...base, table: 'issues' }, 'proposed'))).toBe(false);
    expect(store.applySemanticAction(entry({ ...base, column: 'nope', table: 'issues' }))).toBe(
      false,
    );
    expect(
      store.applySemanticAction(entry({ kind: 'pin_insight', source_id: 's', table: 'issues' })),
    ).toBe(false);
    expect(store.columnByName('units')?.label).toBeNull();
  });
});

describe('applySemanticAction for table settings', () => {
  test('a new analysis period on the loaded table lands in the store', () => {
    const store = load([col('created_at', 'DateTime', { role: 'time' })]);
    const base = { kind: 'set_table_settings', source_id: 's' };
    expect(
      store.applySemanticAction(entry({ ...base, table: 'issues', time_granularity: 'month' })),
    ).toBe(true);
    expect(store.timeGranularity).toBe('month');
    // Another table, or a settings write without a period, changes nothing.
    expect(
      store.applySemanticAction(entry({ ...base, table: 'other', time_granularity: 'day' })),
    ).toBe(false);
    // A display name or description lands too; a blank one clears.
    expect(store.applySemanticAction(entry({ ...base, display_name: 'X', table: 'issues' }))).toBe(
      true,
    );
    expect(store.displayName).toBe('X');
    expect(store.applySemanticAction(entry({ ...base, description: ' ', table: 'issues' }))).toBe(
      true,
    );
    expect(store.description).toBeNull();
    expect(store.timeGranularity).toBe('month');
  });
});
