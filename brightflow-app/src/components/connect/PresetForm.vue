<script setup lang="ts">
import { ref } from 'vue';

defineProps<{
  connectorName: string;
}>();

const emit = defineEmits<{
  submit: [data: { name: string; token: string; config: Record<string, string> }];
  cancel: [];
}>();

const name = ref('');
const token = ref('');
const configEntries = ref<{ key: string; value: string }[]>([]);

function addConfigEntry(): void {
  configEntries.value.push({ key: '', value: '' });
}

function removeConfigEntry(index: number): void {
  configEntries.value.splice(index, 1);
}

function handleSubmit(): void {
  const config: Record<string, string> = {};
  for (const entry of configEntries.value) {
    if (entry.key.trim()) {
      config[entry.key.trim()] = entry.value;
    }
  }
  emit('submit', { name: name.value.trim(), token: token.value, config });
}
</script>

<template>
  <form class="space-y-3" @submit.prevent="handleSubmit">
    <!-- Preset name -->
    <div>
      <label class="mb-1 block text-xs font-medium text-muted">Preset Name</label>
      <input
        v-model="name"
        type="text"
        :placeholder="`${connectorName}-default`"
        class="placeholder-muted w-full rounded border border-default bg-elevated px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
      />
    </div>

    <!-- Token -->
    <div>
      <label class="mb-1 block text-xs font-medium text-muted">API Token</label>
      <input
        v-model="token"
        type="password"
        placeholder="Paste API token..."
        class="placeholder-muted w-full rounded border border-default bg-elevated px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
      />
    </div>

    <!-- Config key-value pairs -->
    <div>
      <div class="mb-1 flex items-center justify-between">
        <label class="text-xs font-medium text-muted">Config</label>
        <button
          type="button"
          class="cursor-pointer text-xs text-blue-500 hover:text-blue-400"
          @click="addConfigEntry"
        >
          + Add field
        </button>
      </div>
      <div v-for="(entry, i) in configEntries" :key="i" class="mb-1.5 flex gap-2">
        <input
          v-model="entry.key"
          type="text"
          placeholder="key"
          class="placeholder-muted w-1/3 rounded border border-default bg-elevated px-2 py-1.5 text-xs text-highlighted focus:border-blue-500 focus:outline-none"
        />
        <input
          v-model="entry.value"
          type="text"
          placeholder="value"
          class="placeholder-muted flex-1 rounded border border-default bg-elevated px-2 py-1.5 text-xs text-highlighted focus:border-blue-500 focus:outline-none"
        />
        <button
          type="button"
          class="cursor-pointer text-xs text-red-400 hover:text-red-300"
          @click="removeConfigEntry(i)"
        >
          Remove
        </button>
      </div>
    </div>

    <!-- Actions -->
    <div class="flex justify-end gap-2 pt-1">
      <UButton type="button" variant="ghost" size="sm" @click="emit('cancel')">Cancel</UButton>
      <UButton type="submit" size="sm" :disabled="!name.trim()">Create Preset</UButton>
    </div>
  </form>
</template>
