/**
 * Stable color palette for topic clusters. Indexed by display order
 * (clusters are sorted size-desc, so index 0 is the largest cluster).
 *
 * Colors are picked to be distinguishable on both light and dark themes
 * and to be roughly color-blind safe.
 */
export const CLUSTER_COLORS = [
  '#7c3aed', // Violet 600
  '#10b981', // Emerald 500
  '#f59e0b', // Amber 500
  '#06b6d4', // Cyan 500
  '#ec4899', // Pink 500
  '#3b82f6', // Blue 500
  '#84cc16', // Lime 500
  '#f43f5e', // Rose 500
  '#14b8a6', // Teal 500
  '#a855f7', // Purple 500
  '#eab308', // Yellow 500
  '#0ea5e9', // Sky 500
  '#22c55e', // Green 500
  '#ef4444', // Red 500
  '#6366f1', // Indigo 500
];

export function clusterColor(index: number): string {
  if (CLUSTER_COLORS.length === 0) {
    return '#7c3aed';
  }
  const safe = Math.max(0, Math.floor(index));
  return CLUSTER_COLORS[safe % CLUSTER_COLORS.length] ?? '#7c3aed';
}
