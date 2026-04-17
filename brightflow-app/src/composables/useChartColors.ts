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
