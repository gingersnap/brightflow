<script setup lang="ts">
/**
 * Inline name-edit widget shared by the source settings pages.
 *
 * One component because the web and connector settings pages need the exact
 * same edit affordance — the previous two hand-rolled copies had already
 * drifted a blue focus ring onto the purple theme.
 */

const model = defineModel<string>({ required: true });

defineProps<{
  saving: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  save: [];
  cancel: [];
}>();
</script>

<template>
  <div class="space-y-2">
    <UInput
      v-model="model"
      type="text"
      class="w-full"
      @keyup.enter="emit('save')"
      @keyup.escape="emit('cancel')"
    />
    <div v-if="error" class="text-sm text-red-500">{{ error }}</div>
    <div class="flex justify-end gap-2">
      <UButton variant="ghost" size="md" icon="i-lucide-x" @click="emit('cancel')">Cancel</UButton>
      <UButton
        size="md"
        icon="i-lucide-check"
        :loading="saving"
        :disabled="!model.trim()"
        @click="emit('save')"
      >
        Save
      </UButton>
    </div>
  </div>
</template>
