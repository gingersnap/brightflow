<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { computed, ref } from 'vue';

import { enrichFnApi } from '@/services/api';
import type { FunctionVersion, LlmPromptConfig } from '@/types/enrichment';

import { diffLines } from './promptDiff';

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

function promptOf(version: FunctionVersion): string {
  const config = version.config as Partial<LlmPromptConfig> | null;
  return typeof config?.prompt_template === 'string' ? config.prompt_template : '';
}

const currentPrompt = computed(() => {
  const current = (versions.value ?? []).find((v) => v.version === props.currentVersion);
  return current ? promptOf(current) : '';
});

function diffFor(version: FunctionVersion) {
  return diffLines(promptOf(version), currentPrompt.value);
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
            <template v-if="version.version !== currentVersion">
              <p class="text-sm text-muted">Prompt diff vs current:</p>
              <!-- Dense diff grid: 12px is intentional here -->
              <pre class="overflow-x-auto rounded bg-muted/10 p-2 text-xs">
<span
  v-for="(line, i) in diffFor(version)"
  :key="i"
  class="block"
  :class="{
    'bg-red-500/10 text-red-500': line.type === 'del',
    'bg-green-500/10 text-green-600': line.type === 'add',
  }"
>{{ line.type === 'del' ? '- ' : line.type === 'add' ? '+ ' : '  ' }}{{ line.text }}</span></pre>
              <UButton
                size="md"
                color="neutral"
                variant="soft"
                icon="i-lucide-history"
                @click="emit('restore', version.config)"
              >
                Restore as new version
              </UButton>
            </template>
            <pre v-else class="overflow-x-auto rounded bg-muted/10 p-2 text-xs">{{
              promptOf(version)
            }}</pre>
          </div>
        </div>
      </div>
    </template>
  </USlideover>
</template>
