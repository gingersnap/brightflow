/**
 * Unit tests for the column context menu builder: which groups appear for
 * which role, that the current role and polarity are checked, that the
 * clear entries show only when there is something to clear, and that a
 * resolved column leads with who resolved it.
 */

import type { ContextMenuItem } from '@nuxt/ui';
import { describe, expect, test, vi } from 'vitest';

import type { ColumnInfo, ColumnRole, Polarity } from '@/types/generated';

import { type ColumnMenuHandlers, columnMenuItems } from './columnMenu';

function handlers(): ColumnMenuHandlers {
  return {
    clearDescription: vi.fn((): void => {}),
    clearLabel: vi.fn((): void => {}),
    describe: vi.fn((): void => {}),
    rename: vi.fn((): void => {}),
    setKpi: vi.fn((_isKpi: boolean): void => {}),
    setPolarity: vi.fn((_polarity: Polarity): void => {}),
    setRole: vi.fn((_role: ColumnRole): void => {}),
  };
}

function col(extra: Partial<ColumnInfo> = {}): ColumnInfo {
  return {
    datatype: 'Float',
    dtype: 'f64',
    isKpi: null,
    label: null,
    name: 'revenue',
    role: null,
    ...extra,
  };
}

function labels(groups: ContextMenuItem[][]): string[][] {
  return groups.map((g) => g.map((i) => String(i.label)));
}

/** A submenu's entries, whichever nesting shape Nuxt UI's type allows. */
function childrenOf(item: ContextMenuItem | undefined): ContextMenuItem[] {
  return (item?.children ?? []).flat();
}

function select(item: ContextMenuItem | undefined): void {
  item?.onSelect?.(new Event('select'));
}

describe('columnMenuItems', () => {
  test('a measure gets role, KPI and polarity, and the text group', () => {
    const h = handlers();
    const column = col({ isKpi: true, polarity: 'lower_is_better', role: 'measure' });
    const groups = columnMenuItems(column, h);
    expect(labels(groups)).toEqual([['Role'], ['Unset KPI', 'Polarity'], ['Rename…', 'Describe…']]);

    const roles = childrenOf(groups[0]?.[0]);
    expect(roles.find((c) => c.label === 'Measure')?.checked).toBe(true);
    expect(roles.find((c) => c.label === 'Time')?.checked).toBe(false);
    const polarities = childrenOf(groups[1]?.[1]);
    expect(polarities.find((c) => c.label === 'Lower is better')?.checked).toBe(true);

    select(groups[1]?.[0]);
    expect(h.setKpi).toHaveBeenCalledWith(false);
    select(roles.find((c) => c.label === 'Time'));
    expect(h.setRole).toHaveBeenCalledWith('time');
  });

  test('a dimension or roleless column has no KPI or polarity entries', () => {
    const dimension = columnMenuItems(col({ role: 'dimension' }), handlers());
    expect(labels(dimension)).toEqual([['Role'], ['Rename…', 'Describe…']]);
    const roleless = columnMenuItems(col(), handlers());
    expect(labels(roleless)).toEqual([['Role'], ['Rename…', 'Describe…']]);
  });

  test('a resolved column leads with a disabled attribution line', () => {
    const column = col({
      resolvedBy: { layer: 'declared', producer: 'connector:github', version: '0.3.0' },
      role: 'dimension',
    });
    const groups = columnMenuItems(column, handlers());
    expect(labels(groups)[0]).toEqual(['From connector github 0.3.0']);
    expect(groups[0]?.[0]?.disabled).toBe(true);
  });

  test('clear entries appear only when a label or description is set', () => {
    const column = col({ description: 'Money in', label: 'Revenue (SEK)' });
    const groups = columnMenuItems(column, handlers());
    expect(labels(groups)[1]).toEqual(['Rename…', 'Clear label', 'Describe…', 'Clear description']);
  });
});
