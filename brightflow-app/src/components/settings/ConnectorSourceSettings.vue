<script setup lang="ts">
import { computed } from 'vue';

import SchedulesPanel from '@/components/connect/SchedulesPanel.vue';
import type { UnifiedSource } from '@/types';

const props = defineProps<{
  source: UnifiedSource;
}>();

const connectorName = computed(() => props.source.connectorName ?? undefined);
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-6">
    <div class="mx-auto w-full max-w-3xl space-y-6">
      <!-- Name -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Name</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <p class="text-sm text-highlighted">{{ source.name }}</p>
          <p v-if="source.connectorName" class="mt-0.5 text-xs text-muted">
            {{ source.connectorName }}
          </p>
        </div>
      </section>

      <!-- Connector config -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
          Connector Config
        </h3>
        <div class="rounded-lg border border-default bg-elevated p-4 text-xs text-muted">
          TODO: connector-specific config (e.g. owner/repo).
        </div>
      </section>

      <!-- Token -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Token</h3>
        <div class="rounded-lg border border-default bg-elevated p-4 text-xs text-muted">
          TODO: token management.
        </div>
      </section>

      <!-- Schedules + runs for this connector -->
      <SchedulesPanel :connector-name="connectorName" />

      <!-- Danger zone -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Danger zone</h3>
        <div class="rounded-lg border border-default bg-elevated p-4 text-xs text-muted">
          TODO: delete source.
        </div>
      </section>
    </div>
  </div>
</template>
