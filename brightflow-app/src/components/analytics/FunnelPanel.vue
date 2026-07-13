<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { ref, computed } from 'vue';

import { productAnalyticsApi } from '@/services/api';

const props = defineProps<{
  sourceId: string;
  period: string;
  eventNames: string[];
}>();

const steps = ref<{ name: string }[]>([{ name: '' }, { name: '' }]);
const windowDays = ref(7);
const submitted = ref(false);

function addStep(): void {
  steps.value.push({ name: '' });
}

function removeStep(index: number): void {
  if (steps.value.length > 2) {
    steps.value.splice(index, 1);
  }
}

const validSteps = computed(() => steps.value.filter((s) => s.name.length > 0));

function runFunnel(): void {
  submitted.value = true;
}

const { data: funnelResult } = useQuery({
  key: () => [
    'pa-funnel',
    props.sourceId,
    props.period,
    validSteps.value.map((s) => s.name).join(','),
    windowDays.value,
    submitted.value,
  ],
  query: async () => {
    if (!submitted.value || validSteps.value.length < 2) {
      return null;
    }
    return await productAnalyticsApi.funnel(props.sourceId, {
      steps: validSteps.value,
      windowSeconds: windowDays.value * 86_400,
      period: props.period,
    });
  },
  enabled: () => submitted.value && validSteps.value.length >= 2,
});
</script>

<template>
  <div>
    <!-- Funnel builder -->
    <div class="mb-6 space-y-3">
      <div v-for="(step, i) in steps" :key="i" class="flex items-center gap-2">
        <span class="w-6 text-center text-xs font-medium text-muted">{{ i + 1 }}</span>
        <USelect
          v-model="step.name"
          :items="eventNames"
          placeholder="Select event..."
          class="flex-1"
        />
        <UButton
          v-if="steps.length > 2"
          size="md"
          variant="ghost"
          color="neutral"
          icon="i-lucide-trash-2"
          aria-label="Remove step"
          @click="removeStep(i)"
        />
      </div>
      <div class="flex items-center gap-3">
        <UButton size="md" variant="ghost" icon="i-lucide-plus" @click="addStep">Add step</UButton>
        <div class="flex items-center gap-2 text-sm text-muted">
          <span>Window:</span>
          <USelect
            v-model="windowDays"
            :items="[
              { label: '1 day', value: 1 },
              { label: '7 days', value: 7 },
              { label: '14 days', value: 14 },
              { label: '30 days', value: 30 },
            ]"
            value-key="value"
            class="w-32"
          />
        </div>
        <UButton size="md" :disabled="validSteps.length < 2" @click="runFunnel">Analyze</UButton>
      </div>
    </div>

    <!-- Funnel results -->
    <div v-if="funnelResult && funnelResult.steps.length > 0" class="space-y-2">
      <div v-for="(step, i) in funnelResult.steps" :key="step.name" class="flex items-center gap-3">
        <span class="w-6 text-center text-xs font-medium text-muted">{{ i + 1 }}</span>
        <div class="flex-1">
          <div class="mb-1 flex items-center justify-between text-sm">
            <span class="text-highlighted">{{ step.name }}</span>
            <span class="text-muted">
              {{ Number(step.count).toLocaleString() }}
              <span class="ml-1 text-xs"> ({{ (step.conversionRate * 100).toFixed(1) }}%) </span>
            </span>
          </div>
          <div class="h-2 overflow-hidden rounded-full bg-elevated">
            <div
              class="h-full rounded-full bg-primary-500 transition-all"
              :style="{ width: `${step.conversionRate * 100}%` }"
            />
          </div>
          <div v-if="i > 0" class="mt-0.5 text-right text-sm text-muted">
            {{ (step.dropoffRate * 100).toFixed(1) }}% drop-off
          </div>
        </div>
      </div>
    </div>
    <p v-else-if="submitted && validSteps.length >= 2" class="py-8 text-center text-sm text-muted">
      No funnel data for this configuration
    </p>
    <p v-else class="py-8 text-center text-sm text-muted">
      Select at least 2 events to build a funnel
    </p>
  </div>
</template>
