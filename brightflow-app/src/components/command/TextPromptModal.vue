<script setup lang="ts">
import { ref } from 'vue';

/**
 * Text-input overlay for command-palette flows. Opened via useOverlay; the
 * overlay promise resolves with the trimmed value, or null when cancelled
 * (empty input counts as cancel — no action takes an empty string).
 */
const props = defineProps<{
  title: string;
  description?: string;
  placeholder?: string;
  initialValue?: string;
  confirmLabel?: string;
}>();

const emit = defineEmits<{ close: [value: string | null] }>();

const value = ref(props.initialValue ?? '');

function confirm(): void {
  const trimmed = value.value.trim();
  emit('close', trimmed === '' ? null : trimmed);
}
</script>

<template>
  <UModal
    :title="title"
    :description="description"
    @update:open="
      (isOpen: boolean) => {
        if (!isOpen) emit('close', null);
      }
    "
  >
    <template #body>
      <!-- Enter confirms (form submit); Esc dismisses via the modal itself. -->
      <form @submit.prevent="confirm">
        <UInput v-model="value" autofocus class="w-full" :placeholder="placeholder" />
      </form>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton size="md" color="neutral" variant="ghost" @click="emit('close', null)">
          Cancel
        </UButton>
        <UButton size="md" color="primary" @click="confirm">
          {{ confirmLabel ?? 'Save' }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>
