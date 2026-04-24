<script setup lang="ts">
import { useColorMode } from '@vueuse/core';
import { Moon, Sun } from 'lucide-vue-next';
import { computed } from 'vue';

import { useUiStore } from '@/stores/ui';
import type { TextSize } from '@/types';

const uiStore = useUiStore();
const colorMode = useColorMode();

const isDark = computed({
  get: () => colorMode.value === 'dark',
  set: (val: boolean) => {
    colorMode.value = val ? 'dark' : 'light';
  },
});

const textSizes: { value: TextSize; label: string; description: string }[] = [
  { value: 'small', label: 'Small', description: '14px — denser UI' },
  { value: 'default', label: 'Default', description: '16px — browser default' },
  { value: 'large', label: 'Large', description: '18px — easier reading' },
];
</script>

<template>
  <UDashboardPanel id="preferences">
    <template #header>
      <UDashboardNavbar title="Preferences">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="p-4">
        <div class="mx-auto max-w-2xl space-y-6">
          <!-- Appearance -->
          <section>
            <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
              Appearance
            </h3>
            <div class="rounded-lg border border-default bg-elevated p-4">
              <div class="flex items-center justify-between">
                <div>
                  <p class="text-sm font-medium text-highlighted">Theme</p>
                  <p class="mt-0.5 text-sm text-muted">
                    Use a dark or light colour scheme for the app.
                  </p>
                </div>
                <div class="flex items-center gap-2">
                  <Sun class="h-4 w-4 text-muted" />
                  <USwitch v-model="isDark" />
                  <Moon class="h-4 w-4 text-muted" />
                </div>
              </div>
            </div>
          </section>

          <!-- Text size -->
          <section>
            <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
              Text size
            </h3>
            <div class="rounded-lg border border-default bg-elevated p-4">
              <p class="mb-3 text-sm text-muted">
                Adjust the overall density of the app. Layout and spacing scale with the text size.
              </p>
              <div class="flex items-center gap-1 rounded-lg bg-muted p-0.5">
                <button
                  v-for="size in textSizes"
                  :key="size.value"
                  class="flex-1 rounded-md px-3 py-1.5 text-sm font-medium transition-colors"
                  :class="
                    uiStore.textSize === size.value
                      ? 'bg-default text-highlighted shadow-sm'
                      : 'cursor-pointer text-muted hover:text-highlighted'
                  "
                  @click="uiStore.setTextSize(size.value)"
                >
                  {{ size.label }}
                </button>
              </div>
              <p class="mt-3 text-sm text-muted">
                {{ textSizes.find((s) => s.value === uiStore.textSize)?.description }}
              </p>
            </div>
          </section>
        </div>
      </div>
    </template>
  </UDashboardPanel>
</template>
