<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { Plus, Trash2 } from 'lucide-vue-next';
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
        <select
          v-model="step.name"
          class="flex-1 rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
        >
          <option value="" disabled>Select event...</option>
          <option v-for="name in eventNames" :key="name" :value="name">{{ name }}</option>
        </select>
        <button
          v-if="steps.length > 2"
          class="rounded p-1 text-muted hover:text-highlighted"
          @click="removeStep(i)"
        >
          <Trash2 class="h-3.5 w-3.5" />
        </button>
      </div>
      <div class="flex items-center gap-3">
        <UButton size="xs" variant="ghost" @click="addStep">
          <Plus class="h-3.5 w-3.5" />
          Add step
        </UButton>
        <div class="flex items-center gap-2 text-xs text-muted">
          <span>Window:</span>
          <select
            v-model.number="windowDays"
            class="rounded border border-default bg-default px-2 py-1 text-xs"
          >
            <option :value="1">1 day</option>
            <option :value="7">7 days</option>
            <option :value="14">14 days</option>
            <option :value="30">30 days</option>
          </select>
        </div>
        <UButton size="sm" :disabled="validSteps.length < 2" @click="runFunnel">Analyze</UButton>
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
          <div v-if="i > 0" class="mt-0.5 text-right text-xs text-muted">
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
