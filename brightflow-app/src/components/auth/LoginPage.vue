<script setup lang="ts">
import { ref } from 'vue';

import { useAuthStore } from '@/stores/auth';

const authStore = useAuthStore();

const email = ref('');
const password = ref('');
const submitting = ref(false);

async function handleSubmit(): Promise<void> {
  if (!email.value || !password.value) {
    return;
  }

  submitting.value = true;
  try {
    await authStore.login(email.value, password.value);
  } catch {
    // Error is set in the store
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="flex flex-1 items-center justify-center bg-default">
    <div class="w-full max-w-sm">
      <div class="mb-8 text-center">
        <h1 class="text-2xl font-semibold text-highlighted">Brightflow</h1>
        <p class="mt-1 text-sm text-muted">Sign in to continue</p>
      </div>

      <form class="space-y-4" @submit.prevent="handleSubmit">
        <UFormField label="Email">
          <UInput
            v-model="email"
            type="email"
            placeholder="you@example.com"
            autocomplete="email"
            required
            class="w-full"
          />
        </UFormField>

        <UFormField label="Password">
          <UInput
            v-model="password"
            type="password"
            placeholder="Password"
            autocomplete="current-password"
            required
            class="w-full"
          />
        </UFormField>

        <p v-if="authStore.error" class="text-sm text-red-500">
          {{ authStore.error }}
        </p>

        <UButton
          type="submit"
          size="md"
          block
          :loading="submitting"
          :disabled="!email || !password"
        >
          Sign in
        </UButton>
      </form>
    </div>
  </div>
</template>
