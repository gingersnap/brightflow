/**
 * Column-semantic edits from anywhere in the UI: role, KPI flag, polarity,
 * label and description, each dispatched through the action bus with the
 * standard toast-and-Undo feedback. Rename and Describe open the same text
 * prompt the command palette uses; a blank submit is a cancel there, so
 * clearing is its own call (`clearLabel`, `clearDescription`), which sends
 * the action without the field — the backend reads an absent field as clear.
 *
 * The dataset store is patched from the resulting `actionEvent`, not here.
 */

import TextPromptModal from '@/components/command/TextPromptModal.vue';
import { useInsightActions } from '@/composables/useInsightActions';
import type { ColumnInfo, ColumnRole, Polarity } from '@/types/generated';
import { POLARITY_LABELS, ROLE_LABELS } from '@/utils/semanticLabels';

export interface ColumnScope {
  sourceId: string;
  table: string;
}

export interface ColumnSemanticEdits {
  setRole: (scope: ColumnScope, column: ColumnInfo, role: ColumnRole) => Promise<void>;
  setKpi: (scope: ColumnScope, column: ColumnInfo, isKpi: boolean) => Promise<void>;
  setPolarity: (scope: ColumnScope, column: ColumnInfo, polarity: Polarity) => Promise<void>;
  /** Prompt for a new label; no-op when the prompt is cancelled. */
  rename: (scope: ColumnScope, column: ColumnInfo) => Promise<void>;
  clearLabel: (scope: ColumnScope, column: ColumnInfo) => Promise<void>;
  /** Prompt for a new description; no-op when the prompt is cancelled. */
  describe: (scope: ColumnScope, column: ColumnInfo) => Promise<void>;
  clearDescription: (scope: ColumnScope, column: ColumnInfo) => Promise<void>;
}

/** A column's shown name: its label, else its raw name. */
function shown(column: ColumnInfo): string {
  return column.label == null || column.label === '' ? column.name : column.label;
}

export function useColumnSemantics(): ColumnSemanticEdits {
  const { dispatchWithFeedback } = useInsightActions();
  const overlay = useOverlay();
  const promptModal = overlay.create(TextPromptModal);

  async function promptText(options: {
    title: string;
    description?: string;
    placeholder?: string;
    initialValue?: string;
    confirmLabel?: string;
  }): Promise<string | null> {
    const result: unknown = await promptModal.open(options).result;
    return typeof result === 'string' ? result : null;
  }

  async function setRole(scope: ColumnScope, column: ColumnInfo, role: ColumnRole): Promise<void> {
    const roleName = ROLE_LABELS[role].toLowerCase();
    await dispatchWithFeedback(
      {
        column: column.name,
        kind: 'set_column_role',
        role,
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: `${shown(column)} is now a ${roleName}`, title: 'Role set' },
    );
  }

  async function setKpi(scope: ColumnScope, column: ColumnInfo, isKpi: boolean): Promise<void> {
    await dispatchWithFeedback(
      {
        column: column.name,
        is_kpi: isKpi,
        kind: 'set_kpi',
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: shown(column), title: isKpi ? 'KPI set' : 'KPI unset' },
    );
  }

  async function setPolarity(
    scope: ColumnScope,
    column: ColumnInfo,
    polarity: Polarity,
  ): Promise<void> {
    const polarityName = POLARITY_LABELS[polarity].toLowerCase();
    await dispatchWithFeedback(
      {
        column: column.name,
        kind: 'set_column_polarity',
        polarity,
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: `${shown(column)}: ${polarityName}`, title: 'Polarity set' },
    );
  }

  async function rename(scope: ColumnScope, column: ColumnInfo): Promise<void> {
    const label = await promptText({
      confirmLabel: 'Rename',
      description: column.name,
      initialValue: column.label ?? '',
      placeholder: 'Shown instead of the column name',
      title: 'Rename column',
    });
    if (label == null) {
      return;
    }
    await dispatchWithFeedback(
      {
        column: column.name,
        kind: 'set_column_label',
        label,
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: `${column.name} → ${label}`, title: 'Column renamed' },
    );
  }

  async function clearLabel(scope: ColumnScope, column: ColumnInfo): Promise<void> {
    await dispatchWithFeedback(
      {
        column: column.name,
        kind: 'set_column_label',
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: column.name, title: 'Label cleared' },
    );
  }

  async function describe(scope: ColumnScope, column: ColumnInfo): Promise<void> {
    const description = await promptText({
      confirmLabel: 'Save',
      description: shown(column),
      initialValue: column.description ?? '',
      placeholder: 'One or two sentences',
      title: 'Describe column',
    });
    if (description == null) {
      return;
    }
    await dispatchWithFeedback(
      {
        column: column.name,
        description,
        kind: 'set_column_description',
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: shown(column), title: 'Description saved' },
    );
  }

  async function clearDescription(scope: ColumnScope, column: ColumnInfo): Promise<void> {
    await dispatchWithFeedback(
      {
        column: column.name,
        kind: 'set_column_description',
        source_id: scope.sourceId,
        table: scope.table,
      },
      { description: shown(column), title: 'Description cleared' },
    );
  }

  return { clearDescription, clearLabel, describe, rename, setKpi, setPolarity, setRole };
}
