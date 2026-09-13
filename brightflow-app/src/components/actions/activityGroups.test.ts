/**
 * Unit tests for the activity feed's run grouping, scope parsing and
 * detail splitting.
 */

import { describe, expect, test } from 'vitest';

import type { ActionLogEntry, AgentRunResponse } from '@/types/generated';

import { groupFeed, kindLabel, pendingIn, scopeOf, splitDetail } from './activityGroups';

function entry(id: number, status: string, agentRunId?: number): ActionLogEntry {
  return {
    actionKind: 'set_column_description',
    actorType: agentRunId == null ? 'human' : 'agent',
    createdAt: 1000 - id,
    id,
    params: null,
    requestId: `r${id}`,
    status,
    undoable: true,
    ...(agentRunId == null ? {} : { agentRunId }),
  };
}

function run(id: number, kind: string, scope: string): AgentRunResponse {
  return {
    createdAt: 0,
    id,
    kind,
    mode: 'propose',
    proposedActions: [],
    scope,
    status: 'completed',
  };
}

describe('groupFeed', () => {
  test('collapses a run into one card where its newest entry sits', () => {
    const feed = [
      entry(9, 'applied'),
      entry(8, 'proposed', 2),
      entry(7, 'proposed', 1),
      entry(6, 'proposed', 2),
      entry(5, 'applied'),
    ];
    const runs = new Map([[2, run(2, 'describe_table', 'describe_table:src:issues')]]);
    const items = groupFeed(feed, runs);
    expect(items.map((i) => i.kind)).toEqual(['entry', 'run', 'run', 'entry']);
    const second = items[1];
    expect(second?.kind === 'run' && second.entries.map((e) => e.id)).toEqual([8, 6]);
    expect(second?.kind === 'run' && second.run?.kind).toBe('describe_table');
    // A run the list did not carry still groups, without its metadata.
    const third = items[2];
    expect(third?.kind === 'run' && third.run).toBeNull();
    expect(third?.kind === 'run' && third.runId).toBe(1);
  });
});

describe('scopeOf', () => {
  test('finds the source and table even when the source id has colons', () => {
    expect(scopeOf(run(1, 'describe_table', 'describe_table:connector:sample:issues'))).toEqual({
      sourceId: 'connector:sample',
      table: 'issues',
    });
    expect(scopeOf(run(1, 'propose_subcategories', 'propose_subcategories:src:issues:42'))).toEqual(
      { sourceId: 'src', table: 'issues' },
    );
    expect(scopeOf(run(1, 'describe_table', 'other:src:issues'))).toBeNull();
    expect(scopeOf(run(1, 'describe_table', 'describe_table:issues'))).toBeNull();
  });
});

describe('splitDetail and pendingIn', () => {
  test('separate the stats line from the closing note', () => {
    expect(splitDetail('12 proposals\nOverruled nothing.')).toEqual({
      note: 'Overruled nothing.',
      stats: '12 proposals',
    });
    expect(splitDetail('0 proposals')).toEqual({ note: '', stats: '0 proposals' });
    expect(splitDetail()).toEqual({ note: '', stats: '' });
    expect(pendingIn([entry(1, 'proposed'), entry(2, 'applied'), entry(3, 'proposed')])).toBe(2);
  });

  test('kind labels read as prose with a fallback', () => {
    expect(kindLabel('describe_table')).toBe('Describe table');
    expect(kindLabel('some_new_kind')).toBe('some new kind');
  });
});
