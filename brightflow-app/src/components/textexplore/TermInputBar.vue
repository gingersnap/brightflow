<script setup lang="ts">
import { ref } from 'vue';

import type { SearchTerm } from '@/types/generated';

defineProps<{
  terms: SearchTerm[];
}>();

const emit = defineEmits<{
  commit: [input: string];
  remove: [index: number];
  pop: [];
}>();

const wholeWord = defineModel<boolean>('wholeWord', { required: true });

const input = ref('');

function handleEnter(): void {
  const value = input.value.trim();
  if (value.length > 0) {
    emit('commit', value);
    input.value = '';
  }
}

function handleBackspace(): void {
  if (input.value.length === 0) {
    emit('pop');
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-3">
      <UInput
        v-model="input"
        class="flex-1"
        icon="i-lucide-text-search"
        placeholder='Filter rows — press Enter to add. Use "quoted phrases", -word to exclude'
        @keydown.enter.prevent="handleEnter"
        @keydown.backspace="handleBackspace"
      />
      <USwitch v-model="wholeWord" label="Whole word" />
    </div>

    <div v-if="terms.length > 0" class="flex flex-wrap items-center gap-2">
      <UBadge
        v-for="(term, i) in terms"
        :key="`${term.exclude ? '-' : '+'}${term.text}`"
        size="lg"
        variant="subtle"
        :color="term.exclude ? 'error' : 'primary'"
      >
        <span>{{ term.exclude ? '−' : '' }}{{ term.text }}</span>
        <!-- Raw button: a UButton inside a badge fights the badge's padding -->
        <button
          type="button"
          class="cursor-pointer opacity-60 hover:opacity-100"
          :aria-label="`Remove ${term.text}`"
          @click="emit('remove', i)"
        >
          <UIcon name="i-lucide-x" class="size-3.5" />
        </button>
      </UBadge>
    </div>
  </div>
</template>
