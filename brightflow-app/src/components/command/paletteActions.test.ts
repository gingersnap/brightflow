/**
 * Unit tests for the column-semantic palette flows: the items each kind
 * builds from the loaded columns, the current-value marker, and the action
 * each leaf dispatches. Insight and vocabulary flows are exercised through
 * the UI and are out of scope here.
 */

import { describe, expect, test, vi } from 'vitest';

import type { Action, ColumnInfo, ModelSummary } from '@/types/generated';

import { ACTION_PALETTE, type PaletteActionContext } from './paletteActions';

function ctx(columns: ColumnInfo[], promptAnswer: string | null = null) {
  const dispatch = vi.fn<(action: Action) => Promise<void>>(() => Promise.resolve());
  const context: PaletteActionContext = {
    data: { categories: [], columns, insights: [] },
    helpers: {
      close: vi.fn((): void => {}),
      confirm: vi.fn(() => Promise.resolve(true)),
      dispatch,
      promptText: vi.fn(() => Promise.resolve(promptAnswer)),
    },
    sourceId: 's',
    table: 'issues',
  };
  return { context, dispatch };
}

const COLUMNS: ColumnInfo[] = [
  {
    datatype: 'Float',
    isKpi: null,
    label: 'Revenue',
    name: 'order_total',
    role: 'measure',
  },
  { datatype: 'String', isKpi: null, label: null, name: 'region', role: null },
];

async function flush(): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}

describe('set_column_role', () => {
  test('lists every column with the five roles, marking the current one', async () => {
    const { context, dispatch } = ctx(COLUMNS);
    const item = ACTION_PALETTE['set_column_role']?.build(context);
    expect(item?.children?.map((c) => c.label)).toEqual(['Revenue', 'region']);
    const roles = item?.children?.[0]?.children ?? [];
    expect(roles.map((r) => r.label)).toEqual([
      'Measure (current)',
      'Dimension',
      'Time',
      'Entity',
      'Ignored',
    ]);
    roles[2]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      column: 'order_total',
      kind: 'set_column_role',
      role: 'time',
      source_id: 's',
      table: 'issues',
    });
  });
});

describe('set_column_label and set_column_description', () => {
  test('prompt, then dispatch the answer; a cancelled prompt dispatches nothing', async () => {
    const { context, dispatch } = ctx(COLUMNS, 'Sales');
    ACTION_PALETTE['set_column_label']?.build(context).children?.[1]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      column: 'region',
      kind: 'set_column_label',
      label: 'Sales',
      source_id: 's',
      table: 'issues',
    });

    const cancelled = ctx(COLUMNS, null);
    ACTION_PALETTE['set_column_description']?.build(cancelled.context).children?.[0]?.onSelect?.();
    await flush();
    expect(cancelled.dispatch).not.toHaveBeenCalled();
  });
});

describe('set_table_settings', () => {
  test('the period submenu dispatches only the granularity; rename prompts for a name', async () => {
    const { context, dispatch } = ctx(COLUMNS, 'Tickets');
    const item = ACTION_PALETTE['set_table_settings']?.build(context);
    expect(item?.children?.map((c) => c.label)).toEqual([
      'Analysis period',
      'Rename table…',
      'Describe table…',
    ]);
    const periods = item?.children?.[0]?.children ?? [];
    expect(periods.map((p) => p.label)).toEqual(['Day', 'Week', 'Month', 'Quarter', 'Year']);
    periods[2]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      kind: 'set_table_settings',
      source_id: 's',
      table: 'issues',
      time_granularity: 'month',
    });

    item?.children?.[1]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      display_name: 'Tickets',
      kind: 'set_table_settings',
      source_id: 's',
      table: 'issues',
    });
  });
});

describe('reset_column_semantics', () => {
  test('offers only edited columns and dispatches the reset', async () => {
    const edited: ColumnInfo = {
      ...COLUMNS[0]!,
      resolvedBy: { layer: 'user', producer: 'user:1' },
    };
    const { context, dispatch } = ctx([edited, COLUMNS[1]!]);
    const item = ACTION_PALETTE['reset_column_semantics']?.build(context);
    expect(item?.children?.map((c) => c.label)).toEqual(['Revenue']);
    item?.children?.[0]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      column: 'order_total',
      kind: 'reset_column_semantics',
      source_id: 's',
      table: 'issues',
    });
  });
});

function modelCtx(models: ModelSummary[], promptAnswer: string | null) {
  const dispatch = vi.fn<(action: Action) => Promise<void>>(() => Promise.resolve());
  const context: PaletteActionContext = {
    data: {
      captureModel: () => ({
        clientSpec: {
          pivot: {
            columnFields: [],
            decimalPlaces: 2,
            rowFields: [],
            showColumnTotals: true,
            showConditionalFormatting: false,
            showSubtotals: true,
            valueFields: [],
          },
          query: {
            filters: [],
            limit: 100,
            sections: {
              filter: { collapsed: false, enabled: true },
              limit: { collapsed: false, enabled: true },
              sort: { collapsed: true, enabled: false },
            },
            sortBy: null,
            sortDescending: false,
          },
          version: 1,
        },
        recipe: { operations: [{ n: 100, type: 'limit' }], version: 1 },
      }),
      categories: [],
      columns: [],
      insights: [],
      models,
    },
    helpers: {
      close: vi.fn((): void => {}),
      confirm: vi.fn(() => Promise.resolve(true)),
      dispatch,
      promptText: vi.fn(() => Promise.resolve(promptAnswer)),
    },
    sourceId: 's',
    table: 'orders',
  };
  return { context, dispatch };
}

describe('model kinds', () => {
  const model: ModelSummary = {
    id: 'm1',
    inputTable: 'orders',
    recipe: { operations: [{ n: 100, type: 'limit' }], version: 1 },
    table: 'eu_orders',
    version: 1,
  };

  test('create_model prompts for a name and dispatches the captured recipe', async () => {
    const { context, dispatch } = modelCtx([], 'top_orders');
    ACTION_PALETTE['create_model']?.build(context).onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'create_model',
        name: 'top_orders',
        recipe: { operations: [{ n: 100, type: 'limit' }], version: 1 },
        source_id: 's',
        table: 'orders',
      }),
    );
  });

  test('rebuild_model and delete_model list the models and scope to the output table', async () => {
    const { context, dispatch } = modelCtx([model], null);
    const rebuild = ACTION_PALETTE['rebuild_model']?.build(context);
    expect(rebuild?.children?.map((c) => c.label)).toEqual(['eu_orders']);
    expect(rebuild?.children?.[0]?.suffix).toBe('from orders');
    rebuild?.children?.[0]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      kind: 'rebuild_model',
      model_id: 'm1',
      source_id: 's',
      table: 'eu_orders',
    });

    ACTION_PALETTE['delete_model']?.build(context).children?.[0]?.onSelect?.();
    await flush();
    expect(dispatch).toHaveBeenCalledWith({
      kind: 'delete_model',
      model_id: 'm1',
      source_id: 's',
      table: 'eu_orders',
    });
    // With no models the pickers have nothing to pick and the palette drops them.
    expect(ACTION_PALETTE['rebuild_model']?.build(modelCtx([], null).context).children).toEqual([]);
  });
});
