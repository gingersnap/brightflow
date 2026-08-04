/**
 * CSV assembly for client-side exports.
 *
 * Pure string building — the download plumbing (Blob, object URL) stays with
 * the component that triggers it.
 */

/** Quote a cell iff it contains a delimiter, quote, or newline. */
export function escapeCsvCell(cell: unknown): string {
  if (cell === null || cell === undefined) {
    return '';
  }
  const str = typeof cell === 'string' ? cell : JSON.stringify(cell);
  if (str.includes(',') || str.includes('"') || str.includes('\n')) {
    return `"${str.replaceAll('"', '""')}"`;
  }
  return str;
}

/** Header row + data rows, newline-joined. */
export function buildCsv(headers: string[], rows: unknown[][]): string {
  const head = headers.map((h) => escapeCsvCell(h)).join(',');
  const body = rows.map((row) => row.map((cell) => escapeCsvCell(cell)).join(','));
  return [head, ...body].join('\n');
}
