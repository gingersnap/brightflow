<script setup lang="ts">
import { Minus, Plus } from '@lucide/vue';
import { computed } from 'vue';

import type { AnalysisNode } from '@/types/generated';
import { displaySegmentValue } from '@/utils/format';

const props = defineProps<{ node: AnalysisNode }>();

const data = computed(() => {
  const d = props.node.data;
  if (!d || d.type !== 'MembershipDiff') {
    return null;
  }
  return d;
});

const MAX_DISPLAY = 8;
</script>

<template>
  <div class="flex flex-col gap-2 sm:flex-row" v-if="data">
    <div class="flex-1 rounded-md border border-emerald-500/20 bg-emerald-500/5 p-3">
      <div class="mb-1.5 flex items-center gap-1.5">
        <Plus class="h-3.5 w-3.5 text-emerald-500" />
        <span class="text-xs tracking-wider text-muted uppercase">
          Added ({{ data.added.length }})
        </span>
      </div>
      <ul class="space-y-0.5">
        <li v-for="v in data.added.slice(0, MAX_DISPLAY)" :key="v" class="font-mono-data text-sm">
          {{ displaySegmentValue(v) }}
        </li>
        <li v-if="data.added.length > MAX_DISPLAY" class="text-xs text-muted">
          + {{ data.added.length - MAX_DISPLAY }} more
        </li>
      </ul>
    </div>
    <div class="flex-1 rounded-md border border-red-500/20 bg-red-500/5 p-3">
      <div class="mb-1.5 flex items-center gap-1.5">
        <Minus class="h-3.5 w-3.5 text-red-500" />
        <span class="text-xs tracking-wider text-muted uppercase">
          Removed ({{ data.removed.length }})
        </span>
      </div>
      <ul class="space-y-0.5">
        <li v-for="v in data.removed.slice(0, MAX_DISPLAY)" :key="v" class="font-mono-data text-sm">
          {{ displaySegmentValue(v) }}
        </li>
        <li v-if="data.removed.length > MAX_DISPLAY" class="text-xs text-muted">
          + {{ data.removed.length - MAX_DISPLAY }} more
        </li>
      </ul>
    </div>
  </div>
  <div v-else class="flex h-32 items-center justify-center text-sm text-muted">
    No membership data available
  </div>
</template>
