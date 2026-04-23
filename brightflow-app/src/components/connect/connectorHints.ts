export interface FieldHint {
  key: string;
  label: string;
  placeholder?: string;
}

export const CONNECTOR_FIELD_HINTS: Record<string, FieldHint[]> = {
  github: [{ key: 'repo', label: 'Repository', placeholder: 'anthropics/claude-code' }],
};

export function hintsFor(connectorName: string): FieldHint[] {
  return CONNECTOR_FIELD_HINTS[connectorName] ?? [];
}
