<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { computed } from 'vue';

import { llmApi } from '@/services/api';
import type { LlmPromptConfig, OutputField, OutputType } from '@/types/enrichment';

import OutputTypePicker from './OutputTypePicker.vue';
import PromptEditor from './PromptEditor.vue';

const props = defineProps<{
  modelValue: LlmPromptConfig;
  columns: string[];
}>();

const emit = defineEmits<{
  'update:modelValue': [value: LlmPromptConfig];
}>();

const { data: providers } = useQuery({
  key: ['llm-providers'],
  query: async () => (await llmApi.listProviders()) ?? [],
});

const providerItems = computed(() =>
  (providers.value ?? []).map((p) => ({
    label: `${p.name} (${p.model})`,
    value: String(p.id),
  })),
);

function patch(partial: Partial<LlmPromptConfig>): void {
  emit('update:modelValue', { ...props.modelValue, ...partial });
}

const promptTemplate = computed({
  get: () => props.modelValue.prompt_template,
  set: (prompt_template: string) => patch({ prompt_template }),
});

const providerId = computed({
  get: () => props.modelValue.provider_id,
  set: (provider_id: string) => patch({ provider_id }),
});

const model = computed({
  get: () => props.modelValue.model ?? '',
  set: (value: string) => patch({ model: value.trim() === '' ? null : value.trim() }),
});

function updateOutput(index: number, partial: Partial<OutputField>): void {
  const outputs = props.modelValue.outputs.map((field, i) =>
    i === index ? { ...field, ...partial } : field,
  );
  patch({ outputs });
}

function updateOutputType(index: number, dtype: OutputType): void {
  updateOutput(index, { dtype });
}

function addOutput(): void {
  patch({
    outputs: [
      ...props.modelValue.outputs,
      { description: '', dtype: { type: 'string' }, name: '' },
    ],
  });
}

function removeOutput(index: number): void {
  patch({ outputs: props.modelValue.outputs.filter((_, i) => i !== index) });
}
</script>

<template>
  <div class="space-y-4">
    <PromptEditor v-model="promptTemplate" :columns="columns" />

    <div class="space-y-2">
      <div class="flex items-center justify-between">
        <p class="text-sm font-medium text-highlighted">Output columns</p>
        <UButton size="xs" color="neutral" variant="soft" icon="i-lucide-plus" @click="addOutput">
          Add output
        </UButton>
      </div>
      <div
        v-for="(field, index) in modelValue.outputs"
        :key="index"
        class="space-y-2 rounded-lg border border-default p-3"
      >
        <div class="flex items-center gap-2">
          <UInput
            :model-value="field.name"
            placeholder="column_name"
            size="md"
            class="w-48 font-mono"
            @update:model-value="updateOutput(index, { name: String($event) })"
          />
          <OutputTypePicker
            :model-value="field.dtype"
            @update:model-value="updateOutputType(index, $event)"
          />
          <UButton
            size="xs"
            color="neutral"
            variant="ghost"
            icon="i-lucide-trash-2"
            class="ml-auto"
            :aria-label="`Remove output ${field.name}`"
            @click="removeOutput(index)"
          />
        </div>
        <UInput
          :model-value="field.description"
          placeholder="What should this column contain? (guides the model)"
          size="md"
          class="w-full"
          @update:model-value="updateOutput(index, { description: String($event) })"
        />
      </div>
      <p v-if="modelValue.outputs.length === 0" class="text-sm text-muted">
        Add at least one output column.
      </p>
    </div>

    <div class="flex items-end gap-3">
      <div class="w-64">
        <p class="mb-1 text-sm font-medium text-highlighted">Provider</p>
        <USelectMenu
          v-model="providerId"
          :items="providerItems"
          value-key="value"
          size="md"
          class="w-full"
          placeholder="Default provider"
        />
      </div>
      <div class="w-52">
        <p class="mb-1 text-sm font-medium text-highlighted">
          Model <span class="font-normal text-muted">(optional)</span>
        </p>
        <UInput v-model="model" placeholder="provider default" size="md" class="w-full" />
      </div>
    </div>
    <p v-if="providerItems.length === 0" class="text-sm text-warning">
      No LLM provider configured — add one under Preferences → LLM first.
    </p>
  </div>
</template>
