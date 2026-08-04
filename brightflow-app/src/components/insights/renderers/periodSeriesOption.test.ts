/** Tests for the shared period-series ECharts scaffold. */

import { describe, expect, test } from 'vitest';

import { periodSeriesOption } from './periodSeriesOption';

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v != null;
}

/** Narrow a nested option field, failing the test on shape drift. */
function record(v: unknown): Record<string, unknown> {
  if (!isRecord(v)) {
    throw new Error('expected an object');
  }
  return v;
}

const shorten = (l: string): string => l.slice(0, 4);

const BASE = {
  labels: ['2026-01', '2026-02'],
  series: [{ data: [1, 2], type: 'bar' }],
  yName: 'Revenue',
};

function axis(option: Record<string, unknown>, key: 'xAxis' | 'yAxis'): Record<string, unknown> {
  return record(option[key]);
}

describe('periodSeriesOption', () => {
  test('builds the standard chrome around the given series', () => {
    const option = periodSeriesOption(BASE);
    expect(option['series']).toBe(BASE.series);
    expect(option['grid']).toMatchObject({ bottom: 24, containLabel: true });
    expect(axis(option, 'xAxis')).toMatchObject({ data: BASE.labels, type: 'category' });
    expect(axis(option, 'yAxis')).toMatchObject({ name: 'Revenue', nameGap: 12, type: 'value' });
    expect(option['legend']).toBeUndefined();
    expect(axis(option, 'yAxis')['scale']).toBeUndefined();
  });

  test('wires the x formatter and rotation into the axis label', () => {
    const option = periodSeriesOption({ ...BASE, rotate: 30, xFormatter: shorten });
    const label = record(axis(option, 'xAxis')['axisLabel']);
    expect(label['formatter']).toBe(shorten);
    expect(label['rotate']).toBe(30);
  });

  test('boundaryGap appears only when requested', () => {
    expect(axis(periodSeriesOption(BASE), 'xAxis')['boundaryGap']).toBeUndefined();
    expect(axis(periodSeriesOption({ ...BASE, boundaryGap: false }), 'xAxis')['boundaryGap']).toBe(
      false,
    );
  });

  test('legend, yScale, gridBottom, and tooltip knobs apply', () => {
    const option = periodSeriesOption({
      ...BASE,
      gridBottom: 52,
      legend: true,
      tooltip: { axisPointer: { type: 'cross' } },
      yScale: true,
    });
    expect(option['legend']).toMatchObject({ bottom: 0 });
    expect(axis(option, 'yAxis')['scale']).toBe(true);
    expect(record(option['grid'])['bottom']).toBe(52);
    expect(record(option['tooltip'])['axisPointer']).toEqual({
      type: 'cross',
    });
    expect(record(option['tooltip'])['trigger']).toBe('axis');
  });

  test('axis overrides win over the built defaults', () => {
    const option = periodSeriesOption({
      ...BASE,
      xAxis: { axisLabel: { fontSize: 9, rotate: 30 }, name: 'Amount', nameLocation: 'middle' },
      yAxis: { axisLabel: { fontSize: 9 }, name: 'Count' },
    });
    expect(axis(option, 'xAxis')['name']).toBe('Amount');
    expect(record(axis(option, 'xAxis')['axisLabel'])['fontSize']).toBe(9);
    expect(axis(option, 'yAxis')['name']).toBe('Count');
  });
});
