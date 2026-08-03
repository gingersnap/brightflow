<script setup lang="ts">
/**
 * Renders server-segmented text runs, wrapping highlighted spans in
 * <mark>. Everything goes through plain interpolation — the runs arrive
 * pre-split, so no HTML is ever parsed on the client.
 */

import type { TextRun } from '@/types/generated';

defineProps<{
  runs: TextRun[];
}>();
</script>

<template>
  <!-- Plain interpolation only: runs come pre-segmented from the server,
       never rendered via v-html. -->
  <template v-for="(run, i) in runs" :key="i">
    <mark v-if="run.hl" class="rounded-xs bg-primary/20 text-inherit">{{ run.t }}</mark>
    <template v-else>{{ run.t }}</template>
  </template>
</template>
