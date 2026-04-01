<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { ref, computed } from 'vue';

import { productAnalyticsApi } from '@/services/api';

const props = defineProps<{
  sourceId: string;
  period: string;
  eventNames: string[];
}>();

const cohortEvent = ref('');
const returnEvent = ref('');
const periodType = ref('week');
const numPeriods = ref(8);
const submitted = ref(false);

function runRetention(): void {
  submitted.value = true;
}

const { data: retentionResult } = useQuery({
  key: () => [
    'pa-retention',
    props.sourceId,
    props.period,
    cohortEvent.value,
    returnEvent.value,
    periodType.value,
    numPeriods.value,
    submitted.value,
  ],
  query: async () => {
    if (!submitted.value || !cohortEvent.value || !returnEvent.value) {
      return null;
    }
    return await productAnalyticsApi.retention(props.sourceId, {
      cohortEvent: cohortEvent.value,
      returnEvent: returnEvent.value,
      periodType: periodType.value,
      numPeriods: numPeriods.value,
      period: props.period,
    });
  },
  enabled: () => submitted.value && cohortEvent.value.length > 0 && returnEvent.value.length > 0,
});

const periodLabels = computed(() => {
  const count = numPeriods.value;
  return Array.from({ length: count }, (_, i) => (i === 0 ? `${periodType.value} 0` : `${i}`));
});

function retentionColor(value: number): string {
  if (value >= 0.8) {
    return 'bg-primary-500 text-white';
  }
  if (value >= 0.6) {
    return 'bg-primary-400 text-white';
  }
  if (value >= 0.4) {
    return 'bg-primary-300 text-white';
  }
  if (value >= 0.2) {
    return 'bg-primary-200';
  }
  if (value > 0) {
    return 'bg-primary-100';
  }
  return 'bg-elevated';
}
</script>

<template>
  <div>
    <!-- Config form -->
    <div class="mb-6 flex flex-wrap items-end gap-3">
      <div>
        <label class="mb-1 block text-xs text-muted">Cohort event</label>
        <select
          v-model="cohortEvent"
          class="rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
        >
          <option value="" disabled>Select...</option>
          <option v-for="name in eventNames" :key="name" :value="name">{{ name }}</option>
        </select>
      </div>
      <div>
        <label class="mb-1 block text-xs text-muted">Return event</label>
        <select
          v-model="returnEvent"
          class="rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
        >
          <option value="" disabled>Select...</option>
          <option v-for="name in eventNames" :key="name" :value="name">{{ name }}</option>
        </select>
      </div>
      <div>
        <label class="mb-1 block text-xs text-muted">Period</label>
        <select
          v-model="periodType"
          class="rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
        >
          <option value="week">Weekly</option>
          <option value="month">Monthly</option>
        </select>
      </div>
      <div>
        <label class="mb-1 block text-xs text-muted">Periods</label>
        <select
          v-model.number="numPeriods"
          class="rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
        >
          <option :value="4">4</option>
          <option :value="6">6</option>
          <option :value="8">8</option>
          <option :value="12">12</option>
        </select>
      </div>
      <UButton size="sm" :disabled="!cohortEvent || !returnEvent" @click="runRetention">
        Analyze
      </UButton>
    </div>

    <!-- Retention matrix -->
    <div v-if="retentionResult && retentionResult.rows.length > 0" class="overflow-x-auto">
      <table class="w-full text-xs">
        <thead>
          <tr class="text-muted">
            <th class="pb-2 text-left font-medium">Cohort</th>
            <th class="pb-2 text-right font-medium">Users</th>
            <th v-for="(label, i) in periodLabels" :key="i" class="pb-2 text-center font-medium">
              {{ label }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in retentionResult.rows" :key="row.cohort">
            <td class="py-1 text-highlighted">{{ row.cohort }}</td>
            <td class="py-1 text-right text-muted">
              {{ Number(row.cohortSize).toLocaleString() }}
            </td>
            <td v-for="(pct, i) in row.periods" :key="i" class="py-1 text-center">
              <span
                class="inline-block min-w-[3rem] rounded px-1.5 py-0.5 text-xs"
                :class="retentionColor(pct)"
              >
                {{ (pct * 100).toFixed(0) }}%
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else-if="submitted" class="py-8 text-center text-sm text-muted">
      No retention data for this configuration
    </p>
    <p v-else class="py-8 text-center text-sm text-muted">
      Select cohort and return events to analyze retention
    </p>
  </div>
</template>
