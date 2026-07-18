<script setup lang="ts">
import { computed } from 'vue';

import type { EnrichFunction } from '@/types/enrichment';

const props = defineProps<{
  fn: EnrichFunction;
}>();

const kindIcon = computed(() => {
  switch (props.fn.kind) {
    case 'llm_prompt': {
      return 'i-lucide-wand-sparkles';
    }
    case 'topic_model': {
      return 'i-lucide-shapes';
    }
    default: {
      return 'i-lucide-tags';
    }
  }
});
</script>

<template>
  <span class="flex items-center gap-1.5">
    <UIcon :name="kindIcon" class="size-4 shrink-0 text-muted" />
    <!-- Version + lifecycle chips: 12px is correct for status chips -->
    <span class="text-xs text-muted">v{{ fn.version }}</span>
    <UBadge :color="fn.status === 'promoted' ? 'success' : 'neutral'" variant="subtle" size="sm">
      {{ fn.status === 'promoted' ? 'Promoted' : 'Draft' }}
    </UBadge>
    <UBadge
      v-if="fn.kind === 'llm_prompt' && (fn.staleRowCount ?? 0) > 0"
      color="warning"
      variant="subtle"
      size="sm"
      :title="`~${fn.staleRowCount} rows not yet computed`"
    >
      ~{{ fn.staleRowCount }} stale
    </UBadge>
    <UIcon
      v-if="fn.activeRunId != null"
      name="i-lucide-loader-circle"
      class="size-3.5 animate-spin text-primary-500"
    />
  </span>
</template>
