<script setup lang="ts">
/**
 * Settings section for LLM providers: list, add, remove, and a per-provider
 * connectivity test. Any OpenAI-compatible chat endpoint qualifies, so the
 * API key is optional (local servers like ollama don't need one) and only
 * name, base URL, and model are required.
 */

import { onMounted, reactive, ref } from 'vue';

import { llmApi } from '@/services/api';
import type { LlmProviderResponse } from '@/types/generated';

const providers = ref<LlmProviderResponse[]>([]);
const loading = ref(false);
const testResults = ref<Record<number, string>>({});
const formOpen = ref(false);
const saving = ref(false);
const errorMessage = ref<string | null>(null);

const form = reactive({
  name: '',
  baseUrl: '',
  apiKey: '',
  model: '',
  isDefault: true,
});

async function refresh(): Promise<void> {
  loading.value = true;
  try {
    providers.value = (await llmApi.listProviders()) ?? [];
  } finally {
    loading.value = false;
  }
}

onMounted(() => void refresh());

async function save(): Promise<void> {
  if (!form.name || !form.baseUrl || !form.model) {
    errorMessage.value = 'Name, base URL, and model are required.';
    return;
  }
  saving.value = true;
  errorMessage.value = null;
  try {
    const result = await llmApi.upsertProvider({
      name: form.name,
      baseUrl: form.baseUrl,
      ...(form.apiKey === '' ? {} : { apiKey: form.apiKey }),
      model: form.model,
      isDefault: form.isDefault,
    });
    providers.value = result ?? providers.value;
    formOpen.value = false;
    form.name = '';
    form.baseUrl = '';
    form.apiKey = '';
    form.model = '';
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Save failed';
  } finally {
    saving.value = false;
  }
}

async function remove(id: number): Promise<void> {
  providers.value = (await llmApi.deleteProvider(id)) ?? providers.value;
}

async function test(id: number): Promise<void> {
  testResults.value = { ...testResults.value, [id]: '…' };
  const result = await llmApi.testProvider(id);
  testResults.value = {
    ...testResults.value,
    [id]: result?.ok === true ? 'Connected ✓' : (result?.error ?? 'Failed'),
  };
}
</script>

<template>
  <section>
    <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">LLM provider</h3>
    <div class="rounded-lg border border-default bg-elevated p-4">
      <p class="mb-3 text-sm text-muted">
        Optional. Any OpenAI-compatible chat endpoint works — ollama, llama.cpp, Mistral, Scaleway,
        Claude-compatible proxies. Powers agent curation (suggest labels, merges, triage).
      </p>

      <p v-if="loading && providers.length === 0" class="text-sm text-muted">Loading…</p>

      <ul v-if="providers.length > 0" class="mb-3 space-y-2">
        <li
          v-for="provider in providers"
          :key="provider.id"
          class="flex items-center justify-between gap-3 rounded-md border border-default p-3"
        >
          <div class="min-w-0">
            <p class="text-sm font-medium text-highlighted">
              {{ provider.name }}
              <span
                v-if="provider.isDefault"
                class="ml-1 rounded bg-primary/10 px-1.5 py-0.5 text-xs text-primary"
                >default</span
              >
            </p>
            <p class="truncate text-sm text-muted">
              {{ provider.baseUrl }} · {{ provider.model }}
              <span v-if="provider.hasApiKey"> · key set</span>
            </p>
            <p v-if="testResults[provider.id]" class="text-xs text-muted">
              {{ testResults[provider.id] }}
            </p>
          </div>
          <div class="flex flex-shrink-0 gap-1">
            <UButton size="xs" color="neutral" variant="soft" @click="test(provider.id)">
              Test
            </UButton>
            <UButton size="xs" color="error" variant="ghost" @click="remove(provider.id)">
              Remove
            </UButton>
          </div>
        </li>
      </ul>

      <div v-if="formOpen" class="space-y-2">
        <UInput v-model="form.name" placeholder="Name (e.g. local-ollama)" size="md" />
        <UInput
          v-model="form.baseUrl"
          placeholder="Base URL (e.g. http://localhost:11434/v1)"
          size="md"
        />
        <UInput v-model="form.apiKey" placeholder="API key (optional)" type="password" size="md" />
        <UInput v-model="form.model" placeholder="Model (e.g. llama3.1:8b)" size="md" />
        <div class="flex items-center gap-2">
          <USwitch v-model="form.isDefault" />
          <span class="text-sm text-muted">Use as default provider</span>
        </div>
        <p v-if="errorMessage" class="text-sm text-red-500">{{ errorMessage }}</p>
        <div class="flex gap-2">
          <UButton size="md" color="primary" :loading="saving" @click="save">Save</UButton>
          <UButton size="md" color="neutral" variant="ghost" @click="formOpen = false">
            Cancel
          </UButton>
        </div>
      </div>
      <UButton
        v-else
        size="md"
        color="neutral"
        variant="soft"
        icon="i-lucide-plus"
        @click="formOpen = true"
      >
        Add provider
      </UButton>
    </div>
  </section>
</template>
