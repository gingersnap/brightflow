<script setup lang="ts">
import { Eye, EyeOff } from 'lucide-vue-next';
import { computed, ref, watch } from 'vue';

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

const name = ref('');
const token = ref('');
const showToken = ref(false);
const hintValues = ref<Record<string, string>>({});
const extraEntries = ref<{ key: string; value: string }[]>([]);

function initFromProps(): void {
  const initial = props.initialValues;
  name.value = initial?.name ?? '';
  token.value = initial?.token ?? '';

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

function addConfigEntry(): void {
  extraEntries.value.push({ key: '', value: '' });
}

function removeConfigEntry(index: number): void {
  extraEntries.value.splice(index, 1);
}

function handleSubmit(): void {
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
  emit('submit', { name: name.value.trim(), token: token.value, config });
}

const submitLabel = computed(() => (props.mode === 'edit' ? 'Save' : 'Create source'));
</script>

<template>
  <form class="space-y-3" autocomplete="off" @submit.prevent="handleSubmit">
    <!-- Name -->
    <div>
      <label class="mb-1 block text-sm font-medium text-muted">Name</label>
      <input
        v-model="name"
        type="text"
        name="brightflow-source-name"
        autocomplete="off"
        data-1p-ignore
        data-lpignore="true"
        :placeholder="`${connectorName}-default`"
        class="placeholder-muted w-full rounded border border-default bg-elevated px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
      />
    </div>

    <!-- Token (create mode only; edit mode manages token separately) -->
    <div v-if="mode === 'create' && requiresToken">
      <label class="mb-1 block text-sm font-medium text-muted">{{ tokenField.label }}</label>
      <div class="relative">
        <input
          v-model="token"
          :type="showToken ? 'text' : 'password'"
          name="brightflow-source-token"
          autocomplete="new-password"
          data-1p-ignore
          data-lpignore="true"
          :placeholder="tokenField.placeholder"
          class="placeholder-muted w-full rounded border border-default bg-elevated px-2.5 py-1.5 pr-8 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
        />
        <button
          type="button"
          class="absolute inset-y-0 right-0 flex cursor-pointer items-center px-2 text-muted hover:text-highlighted"
          :aria-label="showToken ? 'Hide token' : 'Show token'"
          @click="showToken = !showToken"
        >
          <EyeOff v-if="showToken" class="h-4 w-4" />
          <Eye v-else class="h-4 w-4" />
        </button>
      </div>
      <p v-if="tokenField.help" class="mt-1 text-sm text-muted">{{ tokenField.help }}</p>
    </div>

    <!-- Typed hint fields -->
    <div v-for="hint in hints" v-show="isHintVisible(hint)" :key="hint.key">
      <label class="mb-1 block text-sm font-medium text-muted">{{ hint.label }}</label>
      <select
        v-if="hint.options"
        v-model="hintValues[hint.key]"
        class="w-full rounded border border-default bg-elevated px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
      >
        <option v-for="opt in hint.options" :key="opt" :value="opt">{{ opt }}</option>
      </select>
      <input
        v-else
        v-model="hintValues[hint.key]"
        type="text"
        :placeholder="hint.placeholder ?? ''"
        class="placeholder-muted w-full rounded border border-default bg-elevated px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
      />
      <p v-if="hint.helperText" class="mt-1 text-sm text-muted">{{ hint.helperText }}</p>
    </div>

    <!-- Generic config key-value pairs -->
    <div>
      <div class="mb-1 flex items-center justify-between">
        <label class="text-sm font-medium text-muted">
          {{ hints.length > 0 ? 'Additional config' : 'Config' }}
        </label>
        <button
          type="button"
          class="cursor-pointer text-sm text-blue-500 hover:text-blue-400"
          @click="addConfigEntry"
        >
          + Add field
        </button>
      </div>
      <div v-for="(entry, i) in extraEntries" :key="i" class="mb-1.5 flex gap-2">
        <input
          v-model="entry.key"
          type="text"
          placeholder="key"
          class="placeholder-muted w-1/3 rounded border border-default bg-elevated px-2 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
        />
        <input
          v-model="entry.value"
          type="text"
          placeholder="value"
          class="placeholder-muted flex-1 rounded border border-default bg-elevated px-2 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
        />
        <button
          type="button"
          class="cursor-pointer text-sm text-red-400 hover:text-red-300"
          @click="removeConfigEntry(i)"
        >
          Remove
        </button>
      </div>
    </div>

    <!-- Actions -->
    <div class="flex justify-end gap-2 pt-1">
      <UButton type="button" variant="ghost" size="md" @click="emit('cancel')">Cancel</UButton>
      <UButton type="submit" size="md" :disabled="!name.trim()">{{ submitLabel }}</UButton>
    </div>
  </form>
</template>
