<script setup lang="ts">
/**
 * Slideover listing a ticket function's saved versions. Expanding a version
 * shows its configuration (the vocabulary snapshot included, since that is
 * what a version pins), and Restore emits the old config to be saved as a
 * NEW version — history is append-only, never rewound.
 */

import { useQuery } from '@pinia/colada';
import { ref } from 'vue';

import { enrichFnApi } from '@/services/api';
import type { FunctionVersion } from '@/types/enrichment';

const props = defineProps<{
  open: boolean;
  functionId: string;
  currentVersion: number;
}>();

const emit = defineEmits<{
  'update:open': [open: boolean];
  /** Restore = save the old config as a NEW version. */
  restore: [config: unknown];
}>();

const { data: versions, isLoading } = useQuery({
  key: () => ['enrich-fn-versions', props.functionId, props.currentVersion],
  query: async () => (await enrichFnApi.versions(props.functionId)) ?? [],
  enabled: () => props.open,
});

const expandedVersion = ref<number | null>(null);

function pretty(version: FunctionVersion): string {
  return JSON.stringify(version.config, null, 2);
}
</script>

<template>
  <USlideover :open="open" title="Version history" @update:open="emit('update:open', $event)">
    <template #body>
      <div class="space-y-3">
        <p v-if="isLoading" class="text-sm text-muted">Loading…</p>
        <div
          v-for="version in versions ?? []"
          :key="version.version"
          class="rounded-lg border border-default"
        >
          <button
            class="flex w-full cursor-pointer items-center gap-2 p-3 text-left"
            @click="expandedVersion = expandedVersion === version.version ? null : version.version"
          >
            <span class="text-sm font-medium text-highlighted">v{{ version.version }}</span>
            <UBadge
              v-if="version.version === currentVersion"
              color="primary"
              variant="subtle"
              size="sm"
            >
              Current
            </UBadge>
            <span class="ml-auto text-sm text-muted">{{ version.createdAt }}</span>
            <UIcon
              :name="
                expandedVersion === version.version
                  ? 'i-lucide-chevron-up'
                  : 'i-lucide-chevron-down'
              "
              class="size-4 text-muted"
            />
          </button>
          <div
            v-if="expandedVersion === version.version"
            class="space-y-2 border-t border-default p-3"
          >
            <!-- Dense config dump: 12px is intentional here -->
            <pre class="overflow-x-auto rounded bg-muted/10 p-2 text-xs">{{ pretty(version) }}</pre>
            <UButton
              v-if="version.version !== currentVersion"
              size="md"
              color="neutral"
              variant="soft"
              icon="i-lucide-history"
              @click="emit('restore', version.config)"
            >
              Restore as new version
            </UButton>
          </div>
        </div>
      </div>
    </template>
  </USlideover>
</template>
