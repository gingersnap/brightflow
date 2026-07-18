/**
 * Hand-rolled LCS line diff for version-history prompt comparison.
 * Small inputs (prompt templates), so O(n·m) is fine.
 */

export interface DiffLine {
  type: 'same' | 'add' | 'del';
  text: string;
}

export function diffLines(before: string, after: string): DiffLine[] {
  const a = before.split('\n');
  const b = after.split('\n');
  const n = a.length;
  const m = b.length;

  // Lcs[i][j] = LCS length of a[i..] and b[j..]
  const lcs: number[][] = Array.from({ length: n + 1 }, () =>
    Array.from({ length: m + 1 }, () => 0),
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i]![j] =
        a[i] === b[j]
          ? (lcs[i + 1]?.[j + 1] ?? 0) + 1
          : Math.max(lcs[i + 1]?.[j] ?? 0, lcs[i]?.[j + 1] ?? 0);
    }
  }

  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ text: a[i] ?? '', type: 'same' });
      i++;
      j++;
    } else if ((lcs[i + 1]?.[j] ?? 0) >= (lcs[i]?.[j + 1] ?? 0)) {
      out.push({ text: a[i] ?? '', type: 'del' });
      i++;
    } else {
      out.push({ text: b[j] ?? '', type: 'add' });
      j++;
    }
  }
  while (i < n) {
    out.push({ text: a[i] ?? '', type: 'del' });
    i++;
  }
  while (j < m) {
    out.push({ text: b[j] ?? '', type: 'add' });
    j++;
  }
  return out;
}
