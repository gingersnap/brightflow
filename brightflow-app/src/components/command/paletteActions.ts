/**
 * Declarative catalogue of command-palette actions.
 *
 * Each entry owns its own confirm/prompt requirements, so the palette itself
 * stays generic and adding an action never means touching the palette component.
 */

import type {
  Action,
  AnalysisNode,
  ColumnInfo,
  DismissReason,
  TaxonomyCategory,
  TimeGranularity,
} from '@/types/generated';
import { COLUMN_ACTION_KINDS, TABLE_ACTION_KINDS } from '@/utils/actionKinds';

/**
 * Per-action command-palette flows.
 *
 * The backend manifest is the source of truth for which actions exist and
 * what they are called; this map is the frontend's knowledge of how to
 * gather each action's parameters (entity drill-downs, text prompts,
 * confirmations). Manifest kinds without an entry here are silently skipped
 * — a new backend action degrades to "not in the palette yet", never to a
 * broken item.
 *
 * v1 exclusions (deliberate): `redefine_taxonomy_category` and
 * `freeze_taxonomy_category` need the entry's current definition in view
 * (VocabularyTree owns them).
 */

/** One palette item — a structural subset of Nuxt UI's CommandPaletteItem. */
export interface PaletteItem {
  label: string;
  icon?: string;
  suffix?: string;
  placeholder?: string;
  children?: PaletteItem[];
  onSelect?: () => void;
}

/** Entity data the flows pick from — fetched/held by useCommandPalette. */
export interface PaletteData {
  categories: TaxonomyCategory[];
  insights: AnalysisNode[];
  columns: ColumnInfo[];
}

// Deliberately type aliases, not interfaces — interfaces lack implicit index signatures.
// The useOverlay ComponentProps constraint requires one.
// oxlint-disable-next-line typescript/consistent-type-definitions -- see above
export type PromptOptions = {
  title: string;
  description?: string;
  placeholder?: string;
  initialValue?: string;
  confirmLabel?: string;
};

// oxlint-disable-next-line typescript/consistent-type-definitions -- see above
export type ConfirmOptions = {
  title: string;
  description: string;
  confirmLabel?: string;
};

export interface PaletteHelpers {
  /** Dispatch through the curation store; toast + invalidate on success. */
  dispatch: (action: Action) => Promise<void>;
  promptText: (options: PromptOptions) => Promise<string | null>;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  /** Close the palette — every terminating onSelect calls this first. */
  close: () => void;
}

export interface PaletteActionContext {
  sourceId: string;
  table: string;
  data: PaletteData;
  helpers: PaletteHelpers;
}

export interface PaletteActionConfig {
  icon: string;
  /**
   * Display-only shortcut hints, structured for later `extractShortcuts`
   * wiring. v1 registers no per-action shortcuts.
   */
  kbds?: string[];
  /** Item body for this action: a drill-down (children) or a direct flow. */
  build: (ctx: PaletteActionContext) => Pick<PaletteItem, 'children' | 'onSelect' | 'placeholder'>;
}

/** Action kinds offered on the text-analytics route. */
export const VOCABULARY_KINDS: readonly string[] = [
  'define_taxonomy_category',
  'rename_taxonomy_category',
  'delete_taxonomy_category',
];

/** Action kinds offered on the insights route (plus `COLUMN_KINDS`). */
export const INSIGHT_KINDS: readonly string[] = [
  'dismiss_insight',
  'pin_insight',
  'annotate_insight',
  'suppress_target',
];

/** Column- and table-semantic kinds, offered wherever a table is in scope. */
export const COLUMN_KINDS: readonly string[] = [...COLUMN_ACTION_KINDS, ...TABLE_ACTION_KINDS];

const GRANULARITY_OPTIONS: { label: string; value: TimeGranularity }[] = [
  { label: 'Day', value: 'day' },
  { label: 'Week', value: 'week' },
  { label: 'Month', value: 'month' },
  { label: 'Quarter', value: 'quarter' },
  { label: 'Year', value: 'year' },
];

// ── Shared pickers ──────────────────────────────────────────────────────────

function categoryChildren(
  ctx: PaletteActionContext,
  onPick: (category: TaxonomyCategory) => void,
): PaletteItem[] {
  return ctx.data.categories.map((category) => ({
    label: category.name,
    suffix: category.kind,
    icon: 'i-lucide-tag',
    onSelect: () => {
      onPick(category);
    },
  }));
}

function insightChildren(
  ctx: PaletteActionContext,
  build: (insight: AnalysisNode) => Pick<PaletteItem, 'children' | 'onSelect'>,
): PaletteItem[] {
  return ctx.data.insights.map((insight) => ({
    label: insight.summary.length > 80 ? `${insight.summary.slice(0, 80)}…` : insight.summary,
    icon: 'i-lucide-lightbulb',
    ...build(insight),
  }));
}

/** Fire-and-forget wrapper so onSelect handlers stay synchronous. */
function run(flow: () => Promise<void>): void {
  void flow();
}

const POLARITY_ICONS = {
  higher_is_better: 'i-lucide-trending-up',
  lower_is_better: 'i-lucide-trending-down',
  neutral: 'i-lucide-minus',
} as const;

const ROLE_OPTIONS = [
  { icon: 'i-lucide-hash', label: 'Measure', value: 'measure' as const },
  { icon: 'i-lucide-tag', label: 'Dimension', value: 'dimension' as const },
  { icon: 'i-lucide-calendar', label: 'Time', value: 'time' as const },
  { icon: 'i-lucide-user', label: 'Entity', value: 'entity' as const },
  { icon: 'i-lucide-eye-off', label: 'Ignored', value: 'ignored' as const },
];

/** A column's shown name: its label, else its raw name. */
function columnLabel(column: ColumnInfo): string {
  return column.label == null || column.label === '' ? column.name : column.label;
}

// ── The registry ────────────────────────────────────────────────────────────

export const ACTION_PALETTE: Record<string, PaletteActionConfig> = {
  define_taxonomy_category: {
    icon: 'i-lucide-plus',
    build: (ctx) => ({
      onSelect: () => {
        ctx.helpers.close();
        run(async () => {
          const name = await ctx.helpers.promptText({
            title: 'Define taxonomy category',
            description: 'What is WRONG for the user — not how the ticket is written.',
            placeholder: 'e.g. authentication failure',
            confirmLabel: 'Next',
          });
          if (name == null) {
            return;
          }
          const description = await ctx.helpers.promptText({
            title: 'Describe the symptom',
            description: 'One sentence, optional — confirm with an empty field to skip.',
            confirmLabel: 'Define',
          });
          await ctx.helpers.dispatch({
            kind: 'define_taxonomy_category',
            source_id: ctx.sourceId,
            table: ctx.table,
            name,
            description,
          });
        });
      },
    }),
  },

  rename_taxonomy_category: {
    icon: 'i-lucide-pencil-line',
    build: (ctx) => ({
      placeholder: 'Rename which category…',
      children: categoryChildren(ctx, (category) => {
        ctx.helpers.close();
        run(async () => {
          const name = await ctx.helpers.promptText({
            title: 'Rename category',
            initialValue: category.name,
            confirmLabel: 'Rename',
          });
          if (name == null) {
            return;
          }
          await ctx.helpers.dispatch({
            kind: 'rename_taxonomy_category',
            source_id: ctx.sourceId,
            table: ctx.table,
            category_id: category.id,
            name,
          });
        });
      }),
    }),
  },

  delete_taxonomy_category: {
    icon: 'i-lucide-trash-2',
    build: (ctx) => ({
      placeholder: 'Delete which category…',
      children: categoryChildren(ctx, (category) => {
        ctx.helpers.close();
        run(async () => {
          const confirmed = await ctx.helpers.confirm({
            title: 'Delete category',
            description:
              `Delete “${category.name}”? Tickets classified under it keep their ` +
              `current value until the next run. Undo restores the entry.`,
            confirmLabel: 'Delete',
          });
          if (!confirmed) {
            return;
          }
          await ctx.helpers.dispatch({
            kind: 'delete_taxonomy_category',
            source_id: ctx.sourceId,
            table: ctx.table,
            category_id: category.id,
          });
        });
      }),
    }),
  },

  dismiss_insight: {
    icon: 'i-lucide-x',
    build: (ctx) => ({
      placeholder: 'Dismiss which insight…',
      // Two-level drill-down: insight → reason.
      children: insightChildren(ctx, (insight) => ({
        placeholder: 'Why dismiss it…',
        children: (['boring', 'known', 'wrong'] as DismissReason[]).map((reason) => ({
          label: reason.charAt(0).toUpperCase() + reason.slice(1),
          icon: 'i-lucide-x',
          onSelect: () => {
            ctx.helpers.close();
            run(() =>
              ctx.helpers.dispatch({
                kind: 'dismiss_insight',
                source_id: ctx.sourceId,
                table: ctx.table,
                fingerprint: insight.fingerprint,
                reason,
              }),
            );
          },
        })),
      })),
    }),
  },

  pin_insight: {
    icon: 'i-lucide-pin',
    build: (ctx) => ({
      placeholder: 'Pin which insight…',
      children: insightChildren(ctx, (insight) => ({
        onSelect: () => {
          ctx.helpers.close();
          run(() =>
            ctx.helpers.dispatch({
              kind: 'pin_insight',
              source_id: ctx.sourceId,
              table: ctx.table,
              fingerprint: insight.fingerprint,
              pinned: true,
            }),
          );
        },
      })),
    }),
  },

  annotate_insight: {
    icon: 'i-lucide-message-square-plus',
    build: (ctx) => ({
      placeholder: 'Annotate which insight…',
      children: insightChildren(ctx, (insight) => ({
        onSelect: () => {
          ctx.helpers.close();
          run(async () => {
            const note = await ctx.helpers.promptText({
              title: 'Annotate insight',
              description:
                insight.summary.length > 120
                  ? `${insight.summary.slice(0, 120)}…`
                  : insight.summary,
              placeholder: 'Your note…',
              confirmLabel: 'Annotate',
            });
            if (note == null) {
              return;
            }
            await ctx.helpers.dispatch({
              kind: 'annotate_insight',
              source_id: ctx.sourceId,
              table: ctx.table,
              fingerprint: insight.fingerprint,
              note,
            });
          });
        },
      })),
    }),
  },

  suppress_target: {
    icon: 'i-lucide-volume-off',
    build: (ctx) => ({
      placeholder: 'Suppress insights about…',
      children: [
        {
          label: 'A segment',
          icon: 'i-lucide-filter',
          onSelect: () => {
            ctx.helpers.close();
            run(async () => {
              const target = await ctx.helpers.promptText({
                title: 'Suppress segment',
                description: 'Insights about this segment will never surface again.',
                placeholder: 'e.g. state=closed',
                confirmLabel: 'Suppress',
              });
              if (target == null) {
                return;
              }
              await ctx.helpers.dispatch({
                kind: 'suppress_target',
                source_id: ctx.sourceId,
                table: ctx.table,
                target_kind: 'segment',
                target,
              });
            });
          },
        },
        {
          label: 'A column',
          icon: 'i-lucide-columns-3',
          placeholder: 'Suppress which column…',
          children: ctx.data.columns.map((column) => ({
            label: column.label ?? column.name,
            suffix: column.datatype,
            icon: 'i-lucide-columns-3',
            onSelect: () => {
              ctx.helpers.close();
              run(() =>
                ctx.helpers.dispatch({
                  kind: 'suppress_target',
                  source_id: ctx.sourceId,
                  table: ctx.table,
                  target_kind: 'column',
                  target: column.name,
                }),
              );
            },
          })),
        },
      ],
    }),
  },

  set_kpi: {
    icon: 'i-lucide-target',
    build: (ctx) => ({
      placeholder: 'Toggle KPI on which column…',
      children: ctx.data.columns
        .filter((column) => column.role == null || column.role === 'measure')
        .map((column) => {
          const isKpi = column.isKpi ?? false;
          return {
            label: `${isKpi ? 'Unset' : 'Set'} KPI: ${column.label ?? column.name}`,
            suffix: column.datatype,
            icon: 'i-lucide-target',
            onSelect: () => {
              ctx.helpers.close();
              run(() =>
                ctx.helpers.dispatch({
                  kind: 'set_kpi',
                  source_id: ctx.sourceId,
                  table: ctx.table,
                  column: column.name,
                  is_kpi: !isKpi,
                }),
              );
            },
          };
        }),
    }),
  },

  set_column_role: {
    icon: 'i-lucide-shapes',
    build: (ctx) => ({
      placeholder: 'Set the role of which column…',
      children: ctx.data.columns.map((column) => ({
        label: columnLabel(column),
        suffix: column.role ?? column.datatype,
        icon: 'i-lucide-shapes',
        children: ROLE_OPTIONS.map((option) => ({
          label: option.value === column.role ? `${option.label} (current)` : option.label,
          icon: option.icon,
          onSelect: () => {
            ctx.helpers.close();
            run(() =>
              ctx.helpers.dispatch({
                kind: 'set_column_role',
                source_id: ctx.sourceId,
                table: ctx.table,
                column: column.name,
                role: option.value,
              }),
            );
          },
        })),
      })),
    }),
  },

  set_column_label: {
    icon: 'i-lucide-pencil-line',
    build: (ctx) => ({
      placeholder: 'Rename which column…',
      children: ctx.data.columns.map((column) => ({
        label: columnLabel(column),
        suffix: column.label == null ? column.datatype : column.name,
        icon: 'i-lucide-pencil-line',
        onSelect: () => {
          ctx.helpers.close();
          run(async () => {
            const label = await ctx.helpers.promptText({
              title: 'Rename column',
              description: column.name,
              initialValue: column.label ?? '',
              placeholder: 'Shown instead of the column name',
              confirmLabel: 'Rename',
            });
            if (label == null) {
              return;
            }
            await ctx.helpers.dispatch({
              kind: 'set_column_label',
              source_id: ctx.sourceId,
              table: ctx.table,
              column: column.name,
              label,
            });
          });
        },
      })),
    }),
  },

  set_column_description: {
    icon: 'i-lucide-text',
    build: (ctx) => ({
      placeholder: 'Describe which column…',
      children: ctx.data.columns.map((column) => ({
        label: columnLabel(column),
        suffix: column.description ?? column.datatype,
        icon: 'i-lucide-text',
        onSelect: () => {
          ctx.helpers.close();
          run(async () => {
            const description = await ctx.helpers.promptText({
              title: 'Describe column',
              description: columnLabel(column),
              initialValue: column.description ?? '',
              placeholder: 'One or two sentences',
              confirmLabel: 'Save',
            });
            if (description == null) {
              return;
            }
            await ctx.helpers.dispatch({
              kind: 'set_column_description',
              source_id: ctx.sourceId,
              table: ctx.table,
              column: column.name,
              description,
            });
          });
        },
      })),
    }),
  },

  reset_column_semantics: {
    icon: 'i-lucide-undo-2',
    build: (ctx) => ({
      placeholder: 'Reset which column to its declaration…',
      children: ctx.data.columns
        .filter(
          (column) => column.resolvedBy?.layer === 'user' || column.resolvedBy?.layer === 'agent',
        )
        .map((column) => ({
          label: columnLabel(column),
          suffix: column.resolvedBy?.layer === 'agent' ? 'set by an agent' : 'edited',
          icon: 'i-lucide-undo-2',
          onSelect: () => {
            ctx.helpers.close();
            run(() =>
              ctx.helpers.dispatch({
                kind: 'reset_column_semantics',
                source_id: ctx.sourceId,
                table: ctx.table,
                column: column.name,
              }),
            );
          },
        })),
    }),
  },

  set_table_settings: {
    icon: 'i-lucide-settings-2',
    build: (ctx) => ({
      placeholder: 'Table settings…',
      children: [
        {
          icon: 'i-lucide-calendar-range',
          label: 'Analysis period',
          placeholder: 'Bucket the time axis by…',
          children: GRANULARITY_OPTIONS.map((option) => ({
            icon: 'i-lucide-calendar-range',
            label: option.label,
            onSelect: () => {
              ctx.helpers.close();
              run(() =>
                ctx.helpers.dispatch({
                  kind: 'set_table_settings',
                  source_id: ctx.sourceId,
                  table: ctx.table,
                  time_granularity: option.value,
                }),
              );
            },
          })),
        },
        {
          icon: 'i-lucide-pencil-line',
          label: 'Rename table…',
          onSelect: () => {
            ctx.helpers.close();
            run(async () => {
              const displayName = await ctx.helpers.promptText({
                title: 'Rename table',
                description: ctx.table,
                placeholder: 'Shown instead of the table name',
                confirmLabel: 'Rename',
              });
              if (displayName == null) {
                return;
              }
              await ctx.helpers.dispatch({
                kind: 'set_table_settings',
                source_id: ctx.sourceId,
                table: ctx.table,
                display_name: displayName,
              });
            });
          },
        },
        {
          icon: 'i-lucide-text',
          label: 'Describe table…',
          onSelect: () => {
            ctx.helpers.close();
            run(async () => {
              const description = await ctx.helpers.promptText({
                title: 'Describe table',
                description: ctx.table,
                placeholder: 'What one row of this table is',
                confirmLabel: 'Save',
              });
              if (description == null) {
                return;
              }
              await ctx.helpers.dispatch({
                kind: 'set_table_settings',
                source_id: ctx.sourceId,
                table: ctx.table,
                description,
              });
            });
          },
        },
      ],
    }),
  },

  set_column_polarity: {
    icon: 'i-lucide-arrow-up-down',

    build: (ctx) => ({
      placeholder: 'Set polarity on which measure…',
      children: ctx.data.columns
        .filter((column) => column.role == null || column.role === 'measure')
        .map((column) => {
          const current = column.polarity ?? 'neutral';
          const options = [
            { label: 'Higher is better', value: 'higher_is_better' as const },
            { label: 'Lower is better', value: 'lower_is_better' as const },
            { label: 'Neutral', value: 'neutral' as const },
          ];
          return {
            label: column.label ?? column.name,
            suffix: current === 'neutral' ? column.datatype : current.replaceAll('_', ' '),
            icon: 'i-lucide-arrow-up-down',
            children: options.map((option) => ({
              label: option.value === current ? `${option.label} (current)` : option.label,
              icon: POLARITY_ICONS[option.value],
              onSelect: () => {
                ctx.helpers.close();
                run(() =>
                  ctx.helpers.dispatch({
                    kind: 'set_column_polarity',
                    source_id: ctx.sourceId,
                    table: ctx.table,
                    column: column.name,
                    polarity: option.value,
                  }),
                );
              },
            })),
          };
        }),
    }),
  },
};
