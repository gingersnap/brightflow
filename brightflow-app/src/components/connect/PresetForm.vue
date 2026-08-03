<script setup lang="ts">
/**
 * Create/edit form for a connector preset: name, credential, the connector's
 * typed hint fields, and free-form extra key/value config. Emits the
 * assembled config on submit — the parent owns the API call. In edit mode the
 * token field is hidden, since credentials are updated through a separate
 * flow.
 */

import type { FormError } from '@nuxt/ui';
import { computed, reactive, ref, watch } from 'vue';

import { hintsFor, type FieldHint } from './connectorHints';

interface PresetInitialValues {
  name: string;
  token: string;
  config: Record<string, string>;
}

const props = withDefaults(
  defineProps<{
    connectorName: string;
    mode?: 'create' | 'edit';
    initialValues?: PresetInitialValues | null;
  }>(),
  { mode: 'create', initialValues: null },
);

const emit = defineEmits<{
  submit: [data: { name: string; token: string; config: Record<string, string> }];
  cancel: [];
}>();

const hints = computed<FieldHint[]>(() => hintsFor(props.connectorName));

function isHintVisible(hint: FieldHint): boolean {
  if (!hint.showWhen) {
    return true;
  }
  return hintValues.value[hint.showWhen.key] === hint.showWhen.equals;
}

// Connectors that use only public APIs and don't need a secret credential.
const TOKENLESS_CONNECTORS = new Set<string>();
const requiresToken = computed(() => !TOKENLESS_CONNECTORS.has(props.connectorName));

// Credential field copy. Bluesky authenticates with an App Password; everything
// Else uses a plain API token.
const tokenField = computed(() =>
  props.connectorName === 'bluesky'
    ? {
        label: 'App password',
        placeholder: 'xxxx-xxxx-xxxx-xxxx',
        help: 'Create one in Bluesky: Settings → Privacy and Security → App Passwords. Use the app password, not your main password.',
      }
    : { label: 'API token', placeholder: 'Paste API token...', help: '' },
);

const state = reactive({ name: '', token: '' });
const showToken = ref(false);
// Dynamic keys don't fit UForm field-name paths, so hint/extra values stay
// Outside the validated state.
const hintValues = ref<Record<string, string>>({});
const extraEntries = ref<{ key: string; value: string }[]>([]);

function initFromProps(): void {
  const initial = props.initialValues;
  state.name = initial?.name ?? '';
  state.token = initial?.token ?? '';

  const hintKeys = new Set(hints.value.map((h) => h.key));
  const nextHintValues: Record<string, string> = {};
  for (const hint of hints.value) {
    nextHintValues[hint.key] = initial?.config[hint.key] ?? hint.default ?? '';
  }
  hintValues.value = nextHintValues;

  const extras: { key: string; value: string }[] = [];
  if (initial?.config) {
    for (const [k, v] of Object.entries(initial.config)) {
      if (!hintKeys.has(k)) {
        extras.push({ key: k, value: v });
      }
    }
  }
  extraEntries.value = extras;
}

initFromProps();

watch(
  () => [props.connectorName, props.initialValues] as const,
  () => {
    initFromProps();
  },
);

function validate(s: { name: string; token: string }): FormError[] {
  return s.name.trim() ? [] : [{ name: 'name', message: 'Name is required' }];
}

function addConfigEntry(): void {
  extraEntries.value.push({ key: '', value: '' });
}

function removeConfigEntry(index: number): void {
  extraEntries.value.splice(index, 1);
}

function onSubmit(): void {
  const config: Record<string, string> = {};
  for (const hint of hints.value) {
    if (isHintVisible(hint)) {
      const v = hintValues.value[hint.key] ?? '';
      if (v.trim()) {
        config[hint.key] = v;
      }
    }
  }
  for (const entry of extraEntries.value) {
    const key = entry.key.trim();
    if (key) {
      config[key] = entry.value;
    }
  }
  emit('submit', { name: state.name.trim(), token: state.token, config });
}

const submitLabel = computed(() => (props.mode === 'edit' ? 'Save' : 'Create source'));
</script>

<template>
  <UForm
    :state="state"
    :validate="validate"
    class="space-y-3"
    autocomplete="off"
    @submit="onSubmit"
  >
    <!-- Name -->
    <UFormField label="Name" name="name">
      <UInput
        v-model="state.name"
        name="brightflow-source-name"
        autocomplete="off"
        data-1p-ignore
        data-lpignore="true"
        :placeholder="`${connectorName}-default`"
        class="w-full"
      />
    </UFormField>

    <!-- Token (create mode only; edit mode manages token separately) -->
    <UFormField
      v-if="mode === 'create' && requiresToken"
      :label="tokenField.label"
      :help="tokenField.help"
    >
      <UInput
        v-model="state.token"
        :type="showToken ? 'text' : 'password'"
        name="brightflow-source-token"
        autocomplete="new-password"
        data-1p-ignore
        data-lpignore="true"
        :placeholder="tokenField.placeholder"
        class="w-full"
      >
        <template #trailing>
          <UButton
            size="xs"
            variant="link"
            color="neutral"
            :icon="showToken ? 'i-lucide-eye-off' : 'i-lucide-eye'"
            :aria-label="showToken ? 'Hide token' : 'Show token'"
            @click="showToken = !showToken"
          />
        </template>
      </UInput>
    </UFormField>

    <!-- Typed hint fields -->
    <UFormField
      v-for="hint in hints"
      v-show="isHintVisible(hint)"
      :key="hint.key"
      :label="hint.label"
      :help="hint.helperText ?? ''"
    >
      <!-- Split model binding: the indexed read is `string | undefined` under
           noUncheckedIndexedAccess, which the inputs' modelValue rejects. -->
      <USelect
        v-if="hint.options"
        :model-value="hintValues[hint.key] ?? ''"
        :items="hint.options"
        class="w-full"
        @update:model-value="(v: string) => (hintValues[hint.key] = v)"
      />
      <UInput
        v-else
        :model-value="hintValues[hint.key] ?? ''"
        :placeholder="hint.placeholder ?? ''"
        class="w-full"
        @update:model-value="(v: string) => (hintValues[hint.key] = v)"
      />
    </UFormField>

    <!-- Generic config key-value pairs -->
    <div>
      <div class="mb-1 flex items-center justify-between">
        <label class="text-sm font-medium text-muted">
          {{ hints.length > 0 ? 'Additional config' : 'Config' }}
        </label>
        <UButton size="md" variant="link" @click="addConfigEntry">+ Add field</UButton>
      </div>
      <div v-for="(entry, i) in extraEntries" :key="i" class="mb-1.5 flex gap-2">
        <UInput v-model="entry.key" placeholder="key" class="w-1/3" />
        <UInput v-model="entry.value" placeholder="value" class="flex-1" />
        <UButton size="md" variant="ghost" color="error" @click="removeConfigEntry(i)">
          Remove
        </UButton>
      </div>
    </div>

    <!-- Actions -->
    <div class="flex justify-end gap-2 pt-1">
      <UButton type="button" variant="ghost" size="md" @click="emit('cancel')">Cancel</UButton>
      <UButton type="submit" size="md" :disabled="!state.name.trim()">{{ submitLabel }}</UButton>
    </div>
  </UForm>
</template>
