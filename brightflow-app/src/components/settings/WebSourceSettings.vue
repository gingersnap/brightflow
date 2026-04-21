<script setup lang="ts">
import { useMutation } from '@pinia/colada';
import { Copy, Trash2 } from 'lucide-vue-next';
import { ref } from 'vue';
import { useRouter } from 'vue-router';

import { sourceApi } from '@/services/api';
import type { UnifiedSource } from '@/types';

const props = defineProps<{
  source: UnifiedSource;
}>();

const router = useRouter();

const snippetText = ref('');

// Extract raw source id (strip "web:" prefix)
function rawId(id: string): string {
  return id.startsWith('web:') ? id.slice(4) : id;
}

async function loadSnippet(): Promise<void> {
  const result = await sourceApi.snippet(rawId(props.source.id));
  snippetText.value = result?.snippet ?? '';
}

function copySnippet(): void {
  navigator.clipboard.writeText(snippetText.value);
}

const { mutate: deleteSource } = useMutation({
  mutation: async () => {
    await sourceApi.delete(rawId(props.source.id));
    await router.push({ name: 'sources' });
  },
});
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-6">
    <div class="mx-auto w-full max-w-3xl space-y-6">
      <!-- Name -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Name</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <p class="text-sm text-highlighted">{{ source.name }}</p>
          <p v-if="source.domain" class="mt-0.5 text-xs text-muted">{{ source.domain }}</p>
        </div>
      </section>

      <!-- Tracking snippet -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
          Tracking Snippet
        </h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <button
            v-if="!snippetText"
            class="text-xs text-primary-500 underline"
            @click="loadSnippet"
          >
            Show tracking snippet
          </button>
          <template v-else>
            <div class="mb-2 flex items-center justify-between">
              <span class="text-xs font-medium text-muted">
                Add this to your website's &lt;head&gt;
              </span>
              <UButton size="xs" variant="ghost" @click="copySnippet">
                <Copy class="h-3.5 w-3.5" />
              </UButton>
            </div>
            <code class="block rounded bg-default p-2 text-xs text-highlighted">{{
              snippetText
            }}</code>
          </template>
        </div>
      </section>

      <!-- Danger zone -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Danger zone</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <UButton variant="ghost" color="error" size="sm" @click="deleteSource()">
            <Trash2 class="mr-1.5 h-3.5 w-3.5" />
            Delete source
          </UButton>
        </div>
      </section>
    </div>
  </div>
</template>
