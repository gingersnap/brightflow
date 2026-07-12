<script setup lang="ts">
import { Cable, Check, Globe, Table2 } from '@lucide/vue';

import type { UnifiedSource } from '@/types';

defineProps<{
  source: UnifiedSource;
}>();

defineEmits<{
  select: [id: string];
}>();
</script>

<template>
  <button
    class="flex cursor-pointer flex-col gap-2 rounded-xl border border-default bg-elevated p-5 text-left transition-all hover:border-primary-500/50 hover:shadow-md"
    @click="$emit('select', source.id)"
  >
    <div class="flex items-center gap-2">
      <Globe v-if="source.kind === 'web-analytics'" class="h-5 w-5 text-primary-500" />
      <Cable v-else class="h-5 w-5 text-muted" />
      <h3 class="text-sm font-semibold text-highlighted">{{ source.name }}</h3>
    </div>

    <p class="text-sm text-muted">
      {{ source.kind === 'web-analytics' ? source.domain : source.connectorName }}
    </p>

    <div class="mt-auto flex items-center gap-3 pt-2">
      <span v-if="source.tables.length > 0" class="flex items-center gap-1 text-sm text-muted">
        <Table2 class="h-3.5 w-3.5" />
        {{ source.tables.length }} table{{ source.tables.length === 1 ? '' : 's' }}
      </span>
      <span
        v-if="source.ready"
        class="flex items-center gap-1 text-xs text-green-600 dark:text-green-400"
      >
        <Check class="h-3.5 w-3.5" />
        Ready
      </span>
      <span v-else class="text-xs text-yellow-600 dark:text-yellow-400">Pending</span>
    </div>
  </button>
</template>
