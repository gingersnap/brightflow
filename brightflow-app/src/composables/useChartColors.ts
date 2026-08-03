/**
 * Reads the chart palette from CSS custom properties.
 *
 * Colors live in CSS so themes control them in one place; resolving them here
 * lets chart libraries that need literal values stay theme-aware. Cached after
 * first resolve, since `getComputedStyle` forces layout.
 */

const COUNT = 12;
let cached: string[] | null = null;

function resolve(): string[] {
  const style = getComputedStyle(document.documentElement);
  const colors: string[] = [];
  for (let i = 1; i <= COUNT; i++) {
    const raw = style.getPropertyValue(`--color-data-${i}`).trim();
    colors.push(raw || '#888888');
  }
  return colors;
}

export function useChartColors(): string[] {
  cached ??= resolve();
  return cached;
}
