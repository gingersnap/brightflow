<script setup lang="ts">
/**
 * Two-column breakdown table for the web dashboard's grid.
 *
 * One component for the four (pages/sources/browsers/countries) cards — the
 * previous inline copies drifted on padding and empty states. Kept as a plain
 * table rather than UTable: two fixed columns with no sorting/selection is
 * below UTable's complexity floor, and the dashboard grid needs the compact
 * density.
 */

import type { BreakdownRow } from '@/types/generated';

defineProps<{
  title: string;
  /** Header for the name column (e.g. "Page", "Source"). */
  nameLabel: string;
  rows: BreakdownRow[] | null | undefined;
  /** Render an empty name as this (top pages show "/" for the root). */
  emptyName?: string;
}>();
</script>

<template>
  <div class="rounded-lg border border-default bg-elevated p-4">
    <h3 class="mb-3 text-sm font-medium text-highlighted">{{ title }}</h3>
    <table v-if="rows && rows.length > 0" class="w-full text-sm">
      <thead>
        <tr class="border-b border-default text-sm text-muted">
          <th class="pb-2 text-left font-medium">{{ nameLabel }}</th>
          <th class="pb-2 text-right font-medium">Visitors</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.name" class="border-b border-default last:border-0">
          <td class="py-1.5 text-highlighted">{{ row.name || emptyName || row.name }}</td>
          <td class="py-1.5 text-right text-muted">{{ row.visitors.toLocaleString() }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else class="py-4 text-center text-sm text-muted">No data</p>
  </div>
</template>
