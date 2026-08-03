<script setup lang="ts">
/**
 * Draft/promoted switch for an enrichment function, with copy spelling out
 * what promotion means (runs automatically after each sync vs manual only).
 * Emits the intent; the parent performs the actual promote/demote call.
 */

import { computed } from 'vue';

const props = defineProps<{
  status: 'draft' | 'promoted';
  loading?: boolean;
}>();

const emit = defineEmits<{
  toggle: [promoted: boolean];
}>();

const promoted = computed({
  get: () => props.status === 'promoted',
  set: (value: boolean) => emit('toggle', value),
});
</script>

<template>
  <div class="flex items-start gap-3">
    <USwitch v-model="promoted" :disabled="loading" />
    <div>
      <p class="text-sm font-medium text-highlighted">
        {{ promoted ? 'Promoted' : 'Draft' }}
      </p>
      <p class="text-sm text-muted">
        {{
          promoted
            ? 'Runs automatically after each sync for new and changed rows.'
            : 'Only runs when you trigger it. Promote to run on every sync.'
        }}
      </p>
    </div>
  </div>
</template>
