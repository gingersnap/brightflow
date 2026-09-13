/**
 * Unit tests for the sidebar's tool grouping.
 */

import { describe, expect, test } from 'vitest';

import { toolsForSource, type UnifiedSource } from '@/types';

import { groupTools } from './toolGroups';

function source(tools: UnifiedSource['tools']): UnifiedSource {
  return {
    connectorName: null,
    createdAt: '2026-01-01',
    domain: null,
    id: 'connector:x',
    kind: 'connector',
    name: 'x',
    ready: true,
    tables: [],
    tools,
  };
}

describe('groupTools', () => {
  test('arranges a connector source as overview, analyze, data, settings', () => {
    const groups = groupTools(toolsForSource(source(['dashboard', 'explore', 'insights'])));
    expect(groups.map((g) => [g.id, g.heading, g.tools.map((t) => t.id)])).toEqual([
      ['overview', false, ['dashboard']],
      ['analyze', true, ['explore', 'insights', 'saved']],
      ['data', true, ['semantics']],
      ['settings', false, ['settings']],
    ]);
  });

  test('drops a group the source has no tools in and keeps the backend order within one', () => {
    const groups = groupTools(toolsForSource(source(['textexplore', 'explore'])));
    expect(groups.map((g) => g.id)).toEqual(['analyze', 'data', 'settings']);
    expect(groups[0]?.tools.map((t) => t.id)).toEqual(['textexplore', 'explore', 'saved']);
  });
});
