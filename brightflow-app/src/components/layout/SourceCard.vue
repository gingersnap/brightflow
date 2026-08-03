<script setup lang="ts">
/**
 * Clickable summary card for one source: kind icon, a kind-specific
 * subtitle (domain, CSV upload, or connector name), table count, and a
 * Ready/Pending badge. The whole card is the click target; it emits
 * `select` with the source id rather than navigating itself.
 */

import type { UnifiedSource } from '@/types';

defineProps<{
  source: UnifiedSource;
}>();

defineEmits<{
  select: [id: string];
}>();
</script>

<template>
  <UCard
    as="button"
    class="cursor-pointer text-left transition-all hover:border-primary-500/50 hover:shadow-md"
    :ui="{ body: 'flex h-full flex-col gap-2' }"
    @click="$emit('select', source.id)"
  >
    <div class="flex items-center gap-2">
      <UIcon
        v-if="source.kind === 'web-analytics'"
        name="i-lucide-globe"
        class="size-5 text-primary-500"
      />
      <UIcon
        v-else-if="source.kind === 'upload'"
        name="i-lucide-upload"
        class="size-5 text-muted"
      />
      <UIcon v-else name="i-lucide-cable" class="size-5 text-muted" />
      <h3 class="text-sm font-semibold text-highlighted">{{ source.name }}</h3>
    </div>

    <p class="text-sm text-muted">
      {{
        source.kind === 'web-analytics'
          ? source.domain
          : source.kind === 'upload'
            ? 'CSV upload'
            : source.connectorName
      }}
    </p>

    <div class="mt-auto flex items-center gap-3 pt-2">
      <span v-if="source.tables.length > 0" class="flex items-center gap-1 text-sm text-muted">
        <UIcon name="i-lucide-table-2" class="size-3.5" />
        {{ source.tables.length }} table{{ source.tables.length === 1 ? '' : 's' }}
      </span>
      <UBadge v-if="source.ready" icon="i-lucide-check" color="success" variant="subtle" size="md">
        Ready
      </UBadge>
      <UBadge v-else color="warning" variant="subtle" size="md">Pending</UBadge>
    </div>
  </UCard>
</template>
