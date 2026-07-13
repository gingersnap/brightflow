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
      <UFormField label="Cohort event">
        <USelect v-model="cohortEvent" :items="eventNames" placeholder="Select..." class="w-44" />
      </UFormField>
      <UFormField label="Return event">
        <USelect v-model="returnEvent" :items="eventNames" placeholder="Select..." class="w-44" />
      </UFormField>
      <UFormField label="Period">
        <USelect
          v-model="periodType"
          :items="[
            { label: 'Weekly', value: 'week' },
            { label: 'Monthly', value: 'month' },
          ]"
          value-key="value"
          class="w-32"
        />
      </UFormField>
      <UFormField label="Periods">
        <USelect
          v-model="numPeriods"
          :items="[
            { label: '4', value: 4 },
            { label: '6', value: 6 },
            { label: '8', value: 8 },
            { label: '12', value: 12 },
          ]"
          value-key="value"
          class="w-24"
        />
      </UFormField>
      <UButton size="md" :disabled="!cohortEvent || !returnEvent" @click="runRetention">
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
