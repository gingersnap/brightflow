/**
 * Reads the chart palette from CSS custom properties.
 *
 * Colors live in CSS so themes control them in one place; resolving them here
 * lets chart libraries that need literal values stay theme-aware. Backed by
 * `useCssVar` (per composable call), so a theme switch that changes the
 * custom properties re-renders charts instead of leaving them on the palette
 * resolved at first paint.
 */

import { useCssVar } from '@vueuse/core';
import { reactive, watchEffect } from 'vue';

const COUNT = 12;

export function useChartColors(): string[] {
  const el = document.documentElement;
  const vars = Array.from({ length: COUNT }, (_, i) =>
    useCssVar(`--color-data-${i + 1}`, el, { observe: true }),
  );
  // A reactive array keeps the plain string[] contract for consumers while
  // It still re-triggers their computeds when the theme swaps the variables.
  const colors = reactive<string[]>([]);
  watchEffect(() => {
    for (const [i, v] of vars.entries()) {
      colors[i] = (v.value ?? '').trim() || '#888888';
    }
  });
  return colors;
}
