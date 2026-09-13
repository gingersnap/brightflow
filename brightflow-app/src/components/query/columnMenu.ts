/**
 * The column context menu's items, as a pure function of the column's
 * resolved semantics: who they come from as a leading hint, a role picker
 * with the current role checked, KPI and polarity for measures only, rename
 * and describe, clear entries that appear only when there is something to
 * clear, and "Reset to declared" when a person or an agent has edited the
 * column. The component that shows the menu supplies the handlers; nothing
 * here dispatches.
 */

import type { ContextMenuItem } from '@nuxt/ui';

import type { ColumnInfo, ColumnRole, Polarity } from '@/types/generated';
import { POLARITY_LABELS, provenanceLabel, ROLE_LABELS } from '@/utils/semanticLabels';

export interface ColumnMenuHandlers {
  setRole: (role: ColumnRole) => void;
  setKpi: (isKpi: boolean) => void;
  setPolarity: (polarity: Polarity) => void;
  rename: () => void;
  clearLabel: () => void;
  describe: () => void;
  clearDescription: () => void;
  /** Forget every edit so the producers' declarations show again. */
  reset: () => void;
}

const ROLE_ORDER: ColumnRole[] = ['measure', 'dimension', 'time', 'entity', 'ignored'];
const POLARITY_ORDER: Polarity[] = ['higher_is_better', 'lower_is_better', 'neutral'];

const ROLE_ICONS: Record<ColumnRole, string> = {
  dimension: 'i-lucide-tag',
  entity: 'i-lucide-user',
  ignored: 'i-lucide-eye-off',
  measure: 'i-lucide-hash',
  time: 'i-lucide-calendar',
};

const POLARITY_ICONS: Record<Polarity, string> = {
  higher_is_better: 'i-lucide-trending-up',
  lower_is_better: 'i-lucide-trending-down',
  neutral: 'i-lucide-minus',
};

export function columnMenuItems(
  column: ColumnInfo,
  handlers: ColumnMenuHandlers,
): ContextMenuItem[][] {
  const isMeasure = column.role === 'measure';
  const isKpi = column.isKpi === true;
  const polarity = column.polarity ?? 'neutral';

  const hintGroup: ContextMenuItem[] =
    column.resolvedBy == null
      ? []
      : [
          {
            disabled: true,
            icon: 'i-lucide-info',
            label: provenanceLabel(column.resolvedBy),
          },
        ];

  const roleGroup: ContextMenuItem[] = [
    {
      children: ROLE_ORDER.map((role) => ({
        checked: column.role === role,
        icon: ROLE_ICONS[role],
        label: ROLE_LABELS[role],
        onSelect: () => {
          handlers.setRole(role);
        },
        type: 'checkbox' as const,
      })),
      icon: 'i-lucide-shapes',
      label: 'Role',
    },
  ];

  const measureGroup: ContextMenuItem[] = isMeasure
    ? [
        {
          icon: 'i-lucide-target',
          label: isKpi ? 'Unset KPI' : 'Set as KPI',
          onSelect: () => {
            handlers.setKpi(!isKpi);
          },
        },
        {
          children: POLARITY_ORDER.map((value) => ({
            checked: polarity === value,
            icon: POLARITY_ICONS[value],
            label: POLARITY_LABELS[value],
            onSelect: () => {
              handlers.setPolarity(value);
            },
            type: 'checkbox' as const,
          })),
          icon: 'i-lucide-arrow-up-down',
          label: 'Polarity',
        },
      ]
    : [];

  const textGroup: ContextMenuItem[] = [
    { icon: 'i-lucide-pencil-line', label: 'Rename…', onSelect: handlers.rename },
  ];
  if (column.label != null && column.label !== '') {
    textGroup.push({
      icon: 'i-lucide-eraser',
      label: 'Clear label',
      onSelect: handlers.clearLabel,
    });
  }
  textGroup.push({ icon: 'i-lucide-text', label: 'Describe…', onSelect: handlers.describe });
  if (column.description != null && column.description !== '') {
    textGroup.push({
      icon: 'i-lucide-eraser',
      label: 'Clear description',
      onSelect: handlers.clearDescription,
    });
  }

  const edited = column.resolvedBy?.layer === 'user' || column.resolvedBy?.layer === 'agent';
  const resetGroup: ContextMenuItem[] = edited
    ? [{ icon: 'i-lucide-undo-2', label: 'Reset to declared', onSelect: handlers.reset }]
    : [];

  return [hintGroup, roleGroup, measureGroup, textGroup, resetGroup].filter(
    (group) => group.length > 0,
  );
}
