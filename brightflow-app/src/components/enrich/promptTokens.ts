/**
 * Pure helpers for `{{col:Name}}` prompt-template references.
 * Mirrors the Rust `extract_column_refs` parsing (engine function.rs).
 */

/** Column refs in order of first appearance, deduplicated, trimmed. */
export function extractColumnRefs(template: string): string[] {
  const refs: string[] = [];
  let rest = template;
  for (;;) {
    const start = rest.indexOf('{{col:');
    if (start === -1) {
      break;
    }
    const after = rest.slice(start + 6);
    const end = after.indexOf('}}');
    if (end === -1) {
      break;
    }
    const name = after.slice(0, end).trim();
    if (name !== '' && !refs.includes(name)) {
      refs.push(name);
    }
    rest = after.slice(end + 2);
  }
  return refs;
}

/** Insert a `{{col:Name}}` ref at the cursor; returns text + new cursor. */
export function insertColumnRef(
  template: string,
  cursor: number,
  column: string,
): { text: string; cursor: number } {
  const ref = `{{col:${column}}}`;
  const at = Math.max(0, Math.min(cursor, template.length));
  return {
    cursor: at + ref.length,
    text: template.slice(0, at) + ref + template.slice(at),
  };
}
