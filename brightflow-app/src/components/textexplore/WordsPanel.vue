<script setup lang="ts">
import { computed } from 'vue';

import type { WordScore } from '@/types/generated';

const props = defineProps<{
  common: WordScore[];
  distinctive: WordScore[];
}>();

const emit = defineEmits<{
  include: [term: string];
  exclude: [term: string];
}>();

// Distinctive leads; either section disappears when empty (distinctive is
// Always empty without an active filter).
const sections = computed(() =>
  [
    { title: 'Distinctive', words: props.distinctive },
    { title: 'Common', words: props.common },
  ].filter((s) => s.words.length > 0),
);

function handleWordClick(event: MouseEvent, term: string): void {
  if (event.altKey) {
    emit('exclude', term);
  } else {
    emit('include', term);
  }
}
</script>

<template>
  <div class="flex flex-col gap-5">
    <section v-for="section in sections" :key="section.title">
      <h3 class="text-xs font-medium tracking-wider text-muted uppercase">{{ section.title }}</h3>
      <ul class="mt-2 flex flex-col">
        <li
          v-for="word in section.words"
          :key="word.term"
          class="group flex items-center gap-1 py-0.5"
        >
          <button
            type="button"
            class="min-w-0 flex-1 cursor-pointer truncate text-left text-sm text-default hover:text-primary"
            :title="`Filter to rows mentioning “${word.term}” (alt-click to exclude)`"
            @click="handleWordClick($event, word.term)"
          >
            {{ word.term }}
          </button>
          <span class="text-xs text-muted">{{ word.count }}</span>
          <!-- Raw icon button: UButton's smallest size is oversized for this
               dense per-word affordance -->
          <button
            type="button"
            class="cursor-pointer text-muted opacity-0 group-hover:opacity-100 hover:text-error"
            :aria-label="`Exclude ${word.term}`"
            @click="emit('exclude', word.term)"
          >
            <UIcon name="i-lucide-minus" class="size-3.5" />
          </button>
        </li>
      </ul>
    </section>

    <p v-if="sections.length === 0" class="text-sm text-muted">No terms to show for this subset.</p>
  </div>
</template>
