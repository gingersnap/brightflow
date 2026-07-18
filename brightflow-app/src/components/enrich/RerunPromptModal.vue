<script setup lang="ts">
/**
 * Airtable's prompt-edit dialog: saving a changed prompt on a promoted
 * function asks whether existing rows should be recomputed. "Only new rows"
 * leaves old results in place until content changes; "re-run all" recomputes
 * everything under the new prompt.
 */
defineProps<{
  open: boolean;
  rowCount: number | null;
}>();

const emit = defineEmits<{
  'update:open': [open: boolean];
  choose: [rerun: 'missing' | 'all' | 'none'];
}>();
</script>

<template>
  <UModal :open="open" title="Prompt changed" @update:open="emit('update:open', $event)">
    <template #body>
      <p class="text-sm text-default">
        This function is promoted, so the new prompt applies on future syncs automatically. Should
        existing rows be re-run now?
      </p>
    </template>
    <template #footer>
      <div class="flex w-full flex-wrap justify-end gap-2">
        <UButton size="md" color="neutral" variant="ghost" @click="emit('choose', 'none')">
          Save only
        </UButton>
        <UButton size="md" color="neutral" variant="soft" @click="emit('choose', 'missing')">
          Only new rows
        </UButton>
        <UButton size="md" color="primary" @click="emit('choose', 'all')">
          Re-run all{{ rowCount == null ? '' : ` ~${rowCount.toLocaleString()}` }}
        </UButton>
      </div>
    </template>
  </UModal>
</template>
