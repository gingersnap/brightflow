<script setup lang="ts">
import { ref, watch } from 'vue';

import { useCommandPalette } from '@/composables/useCommandPalette';

/**
 * Global Cmd/Ctrl+K command palette: navigation everywhere, plus
 * context-scoped curation actions fed by the backend action manifest.
 * Mounted once, inside the authenticated shell (App.vue).
 */
const open = ref(false);
const searchTerm = ref('');

const { groups } = useCommandPalette(open, () => {
  open.value = false;
});

// UsingInput: the chord must fire even while a text field is focused.
defineShortcuts({
  meta_k: {
    usingInput: true,
    handler: () => {
      open.value = !open.value;
    },
  },
  ctrl_k: {
    usingInput: true,
    handler: () => {
      open.value = !open.value;
    },
  },
});

watch(open, (isOpen) => {
  if (!isOpen) {
    searchTerm.value = '';
  }
});
</script>

<template>
  <UModal v-model:open="open">
    <template #content>
      <UCommandPalette
        v-model:search-term="searchTerm"
        :groups="groups"
        placeholder="Search or run a command…"
        close
        class="h-96"
        @update:open="open = $event"
      />
    </template>
  </UModal>
</template>
