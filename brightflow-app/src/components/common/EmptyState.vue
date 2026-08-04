<script setup lang="ts">
/**
 * Centered muted placeholder for "nothing to show" states — the one copy of
 * the fallback every chart renderer and results view had hand-rolled.
 * Height comes from the caller (class="h-full" / "h-32"), so the component
 * fits both chart bodies and full panels.
 * Deliberately not used where the empty area is itself an affordance
 * (BucketDropzone's drop target).
 */

defineProps<{ message: string; icon?: string; detail?: string }>();
</script>

<template>
  <div class="flex items-center justify-center text-sm text-muted">
    <div class="text-center">
      <UIcon v-if="icon" :name="icon" class="mx-auto mb-4 h-12 w-12 text-muted/50" />
      <div>{{ message }}</div>
      <div v-if="detail" class="mt-1 text-muted/70">{{ detail }}</div>
    </div>
  </div>
</template>
