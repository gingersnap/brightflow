<script setup lang="ts">
defineProps<{
  title?: string;
  /** Classes for the UCollapsible content element (e.g. flex height chains). */
  contentClass?: string;
}>();

const open = defineModel<boolean>('open', { default: true });
</script>

<template>
  <div>
    <!-- Header lives outside UCollapsible: sites put interactive controls in #actions,
         and UCollapsible's default slot is the trigger (as-child), which would nest them. -->
    <div class="flex items-center justify-between bg-muted/30">
      <button
        type="button"
        class="flex flex-1 items-center gap-2 px-4 py-2 text-left transition-colors hover:bg-muted/40"
        @click="open = !open"
      >
        <UIcon
          :name="open ? 'i-lucide-chevron-down' : 'i-lucide-chevron-right'"
          class="size-4 shrink-0 text-muted"
        />
        <slot name="title">
          <h2 class="text-sm font-medium text-default">{{ title }}</h2>
        </slot>
      </button>

      <div v-if="$slots.actions" class="flex items-center gap-3 pr-4" @click.stop>
        <slot name="actions" :open="open" />
      </div>
    </div>

    <UCollapsible
      :open="open"
      class="flex min-h-0 flex-1 flex-col"
      :ui="{ content: contentClass ?? '' }"
    >
      <template #content>
        <slot />
      </template>
    </UCollapsible>
  </div>
</template>
