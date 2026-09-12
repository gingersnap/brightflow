/**
 * Unit tests for the column-semantic palette flows: the items each kind
 * builds from the loaded columns, the current-value marker, and the action
 * each leaf dispatches. Insight and vocabulary flows are exercised through
 * the UI and are out of scope here.
 */

import { describe, expect, test, vi } from 'vitest';

import type { Action, ColumnInfo } from '@/types/generated';

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
  { dtype: 'f64', isKpi: null, label: 'Revenue', name: 'order_total', role: 'measure' },
  { dtype: 'string', isKpi: null, label: null, name: 'region', role: null },
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
