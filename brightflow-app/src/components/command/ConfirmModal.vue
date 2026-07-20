<script setup lang="ts">
/**
 * Confirmation overlay for command-palette flows that are irreversible or
 * cascade (split_cluster, delete_taxonomy_category). Opened via useOverlay;
 * the overlay promise resolves with true only on explicit confirmation.
 */
defineProps<{
  title: string;
  description: string;
  confirmLabel?: string;
}>();

const emit = defineEmits<{ close: [confirmed: boolean] }>();
</script>

<template>
  <UModal
    :title="title"
    :description="description"
    @update:open="
      (isOpen: boolean) => {
        if (!isOpen) emit('close', false);
      }
    "
  >
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton size="md" color="neutral" variant="ghost" @click="emit('close', false)">
          Cancel
        </UButton>
        <UButton size="md" color="error" @click="emit('close', true)">
          {{ confirmLabel ?? 'Confirm' }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>
