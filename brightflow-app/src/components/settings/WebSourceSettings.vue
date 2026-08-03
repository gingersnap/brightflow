<script setup lang="ts">
import { Check, Copy, Pencil, Trash2, X } from '@lucide/vue';
import { useMutation, useQueryCache } from '@pinia/colada';
import { useClipboard } from '@vueuse/core';
import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';

import { sourceApi } from '@/services/api';
import type { UnifiedSource } from '@/types';

const props = defineProps<{
  source: UnifiedSource;
}>();

const router = useRouter();
const queryCache = useQueryCache();

const snippetText = ref('');

// Extract raw source id (strip "web:" prefix)
const rawId = computed(() =>
  props.source.id.startsWith('web:') ? props.source.id.slice(4) : props.source.id,
);

async function loadSnippet(): Promise<void> {
  const result = await sourceApi.snippet(rawId.value);
  snippetText.value = result?.snippet ?? '';
}

const { copy: copyToClipboard } = useClipboard();

function copySnippet(): void {
  void copyToClipboard(snippetText.value);
}

// --- Name editing ---
const editingName = ref(false);
const nameDraft = ref('');
const nameError = ref<string | null>(null);

function startEditName(): void {
  nameDraft.value = props.source.name;
  nameError.value = null;
  editingName.value = true;
}

function cancelEditName(): void {
  editingName.value = false;
  nameError.value = null;
}

const { mutate: saveName, isLoading: savingName } = useMutation({
  mutation: async (name: string) => await sourceApi.update(rawId.value, { name }),
  onSuccess: () => {
    editingName.value = false;
    queryCache.invalidateQueries({ key: ['unified-sources'] });
  },
  onError: (error: unknown) => {
    nameError.value = error instanceof Error ? error.message : 'Failed to save name';
  },
});

function submitName(): void {
  const trimmed = nameDraft.value.trim();
  if (!trimmed) {
    return;
  }
  saveName(trimmed);
}

const { mutate: deleteSource } = useMutation({
  mutation: async () => {
    await sourceApi.delete(rawId.value);
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
          <div v-if="!editingName" class="flex items-center justify-between">
            <div>
              <p class="text-sm text-highlighted">{{ source.name }}</p>
              <p v-if="source.domain" class="mt-0.5 text-sm text-muted">{{ source.domain }}</p>
            </div>
            <UButton variant="ghost" size="md" @click="startEditName">
              <Pencil class="mr-1 h-3.5 w-3.5" />
              Edit
            </UButton>
          </div>
          <div v-else class="space-y-2">
            <input
              v-model="nameDraft"
              type="text"
              class="placeholder-muted w-full rounded border border-default bg-default px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
              @keyup.enter="submitName"
              @keyup.escape="cancelEditName"
            />
            <div v-if="nameError" class="text-sm text-red-500">{{ nameError }}</div>
            <div class="flex justify-end gap-2">
              <UButton variant="ghost" size="md" @click="cancelEditName">
                <X class="mr-1 h-3.5 w-3.5" />
                Cancel
              </UButton>
              <UButton
                size="md"
                :loading="savingName"
                :disabled="!nameDraft.trim()"
                @click="submitName"
              >
                <Check class="mr-1 h-3.5 w-3.5" />
                Save
              </UButton>
            </div>
          </div>
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
            class="text-sm text-primary-500 underline"
            @click="loadSnippet"
          >
            Show tracking snippet
          </button>
          <template v-else>
            <div class="mb-2 flex items-center justify-between">
              <span class="text-sm font-medium text-muted">
                Add this to your website's &lt;head&gt;
              </span>
              <UButton size="md" variant="ghost" @click="copySnippet">
                <Copy class="h-3.5 w-3.5" />
              </UButton>
            </div>
            <code class="block rounded bg-default p-2 text-sm text-highlighted">{{
              snippetText
            }}</code>
          </template>
        </div>
      </section>

      <!-- Danger zone -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Danger zone</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <UButton variant="ghost" color="error" size="md" @click="deleteSource()">
            <Trash2 class="mr-1.5 h-3.5 w-3.5" />
            Delete source
          </UButton>
        </div>
      </section>
    </div>
  </div>
</template>
