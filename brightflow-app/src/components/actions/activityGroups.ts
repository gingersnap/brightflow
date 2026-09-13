/**
 * The activity feed's grouping: entries from the same agent run collapse
 * into one card placed where the run's newest entry sits, so a describe run
 * reads as one thing with its proposals inside rather than forty rows
 * interleaved with everything else. Pure functions over the feed and the
 * runs list; the components render what these return.
 */

import type { ActionLogEntry, AgentRunResponse, Job } from '@/types/generated';

/**
 * One card in the feed: a person's action on its own, a run with its
 * entries, or a background job that finished.
 */
export type FeedItem =
  | { kind: 'entry'; entry: ActionLogEntry }
  | { kind: 'run'; runId: number; run: AgentRunResponse | null; entries: ActionLogEntry[] }
  | { kind: 'job'; job: Job };

/** When an item happened, for ordering: a run card by its newest entry. */
export function itemTime(item: FeedItem): number {
  switch (item.kind) {
    case 'entry': {
      return item.entry.createdAt;
    }
    case 'run': {
      return item.entries[0]?.createdAt ?? 0;
    }
    case 'job': {
      return item.job.finishedAt ?? item.job.startedAt;
    }
  }
}

/**
 * Finished jobs take their place in the log by the time they finished, so a
 * connector sync sits just above the re-declaration it caused. Running jobs
 * belong to the other tab and are left out; an agent run's completion is
 * left out too, since its card already stands for it.
 */
export function mergeJobs(items: FeedItem[], jobs: Job[]): FeedItem[] {
  const finished: FeedItem[] = jobs
    .filter((job) => job.status !== 'running' && job.kind !== 'agent_run')
    .map((job) => ({ job, kind: 'job' }));
  return [...items, ...finished].toSorted((a, b) => itemTime(b) - itemTime(a));
}

/**
 * Group the feed (newest first) by run. A run's card takes the position of
 * its newest entry; entries inside keep feed order. Runs the list did not
 * carry still group, with `run: null`.
 */
export function groupFeed(
  entries: ActionLogEntry[],
  runs: ReadonlyMap<number, AgentRunResponse>,
): FeedItem[] {
  const items: FeedItem[] = [];
  const cards = new Map<number, Extract<FeedItem, { kind: 'run' }>>();
  for (const entry of entries) {
    const runId = entry.agentRunId;
    if (runId == null) {
      items.push({ entry, kind: 'entry' });
    } else {
      const card = cards.get(runId);
      if (card == null) {
        const fresh: Extract<FeedItem, { kind: 'run' }> = {
          entries: [entry],
          kind: 'run',
          run: runs.get(runId) ?? null,
          runId,
        };
        cards.set(runId, fresh);
        items.push(fresh);
      } else {
        card.entries.push(entry);
      }
    }
  }
  return items;
}

const KIND_LABELS: Record<string, string> = {
  describe_table: 'Describe table',
  narrate_insights: 'Summarize insights',
  propose_categories: 'Propose categories',
  propose_feedback_categories: 'Propose feedback categories',
  propose_subcategories: 'Propose subcategories',
  triage_insights: 'Triage insights',
};

/** "Describe table" — the run kind as a person reads it. */
export function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind.replaceAll('_', ' ');
}

/**
 * The (source, table) a run's scope key names. The key is
 * `{kind}:{source}:{table}` with `:{parent}` appended for subcategory
 * runs, and a source id may itself contain colons (`connector:sample`), so
 * the table is the last segment after the kind and any parent are removed.
 */
export function scopeOf(run: AgentRunResponse): { sourceId: string; table: string } | null {
  const prefix = `${run.kind}:`;
  if (!run.scope.startsWith(prefix)) {
    return null;
  }
  let rest = run.scope.slice(prefix.length);
  if (run.kind === 'propose_subcategories') {
    rest = rest.replace(/:\d+$/u, '');
  }
  const cut = rest.lastIndexOf(':');
  if (cut <= 0 || cut === rest.length - 1) {
    return null;
  }
  return { sourceId: rest.slice(0, cut), table: rest.slice(cut + 1) };
}

/**
 * A run's detail is "{stats line}\n{note}": the counts the runner writes,
 * then whatever the model said in its closing text — for a describe run,
 * what it overruled and why.
 */
export function splitDetail(detail = ''): { stats: string; note: string } {
  const newline = detail.indexOf('\n');
  if (newline === -1) {
    return { note: '', stats: detail.trim() };
  }
  return { note: detail.slice(newline + 1).trim(), stats: detail.slice(0, newline).trim() };
}

/** How many of a card's entries still await review. */
export function pendingIn(entries: ActionLogEntry[]): number {
  return entries.filter((e) => e.status === 'proposed').length;
}
