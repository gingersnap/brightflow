<script setup lang="ts">
import { Database, Play } from 'lucide-vue-next';
import { computed, ref, watch } from 'vue';

import { connectApi } from '@/services/api';
import type { AvailableConnectorResponse, PresetInfo } from '@/types';

import PresetForm from './PresetForm.vue';

const SCHEDULE_PRESETS = [
  { label: 'Every 1h', value: 3600 },
  { label: 'Every 6h', value: 21_600 },
  { label: 'Every 12h', value: 43_200 },
  { label: 'Daily', value: 86_400 },
  { label: 'Weekly', value: 604_800 },
];

const props = defineProps<{
  open: boolean;
  available: AvailableConnectorResponse[];
  mode: 'run' | 'schedule';
}>();

const emit = defineEmits<{
  close: [];
  done: [];
}>();

type Step = 'pick-connector' | 'pick-preset' | 'create-preset' | 'pick-interval';

const step = ref<Step>('pick-connector');
const selectedConnector = ref<AvailableConnectorResponse | null>(null);
const selectedPresetId = ref<string | null>(null);
const intervalSecs = ref(86_400);
const submitting = ref(false);
const errorMsg = ref<string | null>(null);

// Reset when dialog opens
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      reset();
    }
  },
);

function reset(): void {
  step.value = 'pick-connector';
  selectedConnector.value = null;
  selectedPresetId.value = null;
  intervalSecs.value = 86_400;
  submitting.value = false;
  errorMsg.value = null;
}

function selectConnector(c: AvailableConnectorResponse): void {
  selectedConnector.value = c;
  step.value = c.presets.length > 0 ? 'pick-preset' : 'create-preset';
}

function pickPreset(preset: PresetInfo): void {
  selectedPresetId.value = preset.id;
  if (props.mode === 'schedule') {
    step.value = 'pick-interval';
  } else {
    void triggerRun(preset.id);
  }
}

function newPreset(): void {
  step.value = 'create-preset';
}

async function handlePresetCreate(data: {
  name: string;
  token: string;
  config: Record<string, string>;
}): Promise<void> {
  if (!selectedConnector.value) {
    return;
  }
  submitting.value = true;
  errorMsg.value = null;

  try {
    const result = await connectApi.createPreset({
      name: data.name || `${selectedConnector.value.name}-default`,
      connectorPath: selectedConnector.value.name,
      configJson: data.config,
      token: data.token || undefined,
    });

    const presetId = (result as { id?: string } | null)?.id;
    if (presetId) {
      if (props.mode === 'schedule') {
        selectedPresetId.value = presetId;
        step.value = 'pick-interval';
      } else {
        await triggerRun(presetId);
      }
    } else {
      emit('done');
      handleClose();
    }
  } catch (error) {
    errorMsg.value = error instanceof Error ? error.message : 'Failed to create preset';
  } finally {
    submitting.value = false;
  }
}

async function triggerRun(presetId: string): Promise<void> {
  submitting.value = true;
  errorMsg.value = null;

  try {
    await connectApi.runPreset(presetId);
    emit('done');
    handleClose();
  } catch (error) {
    errorMsg.value = error instanceof Error ? error.message : 'Failed to trigger run';
  } finally {
    submitting.value = false;
  }
}

async function createSchedule(): Promise<void> {
  if (!selectedPresetId.value) {
    return;
  }
  submitting.value = true;
  errorMsg.value = null;

  try {
    await connectApi.schedulePreset(selectedPresetId.value, intervalSecs.value);
    emit('done');
    handleClose();
  } catch (error) {
    errorMsg.value = error instanceof Error ? error.message : 'Failed to create schedule';
  } finally {
    submitting.value = false;
  }
}

function handleClose(): void {
  reset();
  emit('close');
}

const title = computed(() => {
  const label = props.mode === 'schedule' ? 'New Schedule' : 'New Run';
  switch (step.value) {
    case 'pick-connector': {
      return label;
    }
    case 'pick-preset': {
      return `${selectedConnector.value?.name ?? ''} — Pick Preset`;
    }
    case 'create-preset': {
      return `${selectedConnector.value?.name ?? ''} — New Preset`;
    }
    case 'pick-interval': {
      return `${selectedConnector.value?.name ?? ''} — Schedule Interval`;
    }
    default: {
      return label;
    }
  }
});
</script>

<template>
  <UModal :open="open" @close="handleClose">
    <template #header>
      <h3 class="text-sm font-semibold text-highlighted">{{ title }}</h3>
    </template>

    <template #body>
      <!-- Error -->
      <div v-if="errorMsg" class="mb-3 rounded bg-red-500/10 p-2 text-xs text-red-500">
        {{ errorMsg }}
      </div>

      <!-- Step 1: Pick connector -->
      <div v-if="step === 'pick-connector'" class="space-y-2">
        <button
          v-for="c in available"
          :key="c.name"
          class="flex w-full cursor-pointer items-center gap-3 rounded-lg border border-default px-3 py-2.5 text-left transition-colors hover:bg-elevated"
          @click="selectConnector(c)"
        >
          <Database class="h-5 w-5 shrink-0 text-muted" />
          <div class="min-w-0 flex-1">
            <div class="flex items-baseline gap-2">
              <span class="text-sm font-medium text-highlighted">{{ c.name }}</span>
              <span v-if="c.version" class="text-xs text-muted">v{{ c.version }}</span>
            </div>
            <p v-if="c.description" class="mt-0.5 text-xs text-muted">{{ c.description }}</p>
          </div>
          <span v-if="c.presets.length > 0" class="text-xs text-muted">
            {{ c.presets.length }} preset{{ c.presets.length === 1 ? '' : 's' }}
          </span>
        </button>

        <p v-if="available.length === 0" class="py-4 text-center text-xs text-muted">
          No connectors available
        </p>
      </div>

      <!-- Step 2: Pick preset -->
      <div v-else-if="step === 'pick-preset'" class="space-y-2">
        <button
          v-for="preset in selectedConnector!.presets"
          :key="preset.id"
          class="flex w-full cursor-pointer items-center gap-3 rounded-lg border border-default px-3 py-2.5 text-left transition-colors hover:bg-elevated"
          :disabled="submitting"
          @click="pickPreset(preset)"
        >
          <div class="min-w-0 flex-1">
            <span class="text-sm font-medium text-highlighted">{{ preset.name }}</span>
            <span
              class="ml-2 text-xs"
              :class="preset.hasToken ? 'text-green-500' : 'text-amber-500'"
            >
              {{ preset.hasToken ? 'Token set' : 'No token' }}
            </span>
          </div>
          <Play class="h-4 w-4 text-muted" />
        </button>

        <button
          class="w-full cursor-pointer rounded-lg border border-dashed border-default px-3 py-2.5 text-center text-xs text-muted transition-colors hover:bg-elevated hover:text-highlighted"
          @click="newPreset"
        >
          + Create new preset
        </button>
      </div>

      <!-- Step 3: Create preset form -->
      <div v-else-if="step === 'create-preset'">
        <PresetForm
          :connector-name="selectedConnector!.name"
          @submit="handlePresetCreate"
          @cancel="step = selectedConnector!.presets.length > 0 ? 'pick-preset' : 'pick-connector'"
        />
      </div>

      <!-- Step 4: Pick interval (schedule mode only) -->
      <div v-else-if="step === 'pick-interval'" class="space-y-3">
        <div class="space-y-2">
          <button
            v-for="preset in SCHEDULE_PRESETS"
            :key="preset.value"
            class="flex w-full cursor-pointer items-center justify-between rounded-lg border px-3 py-2.5 text-left text-sm transition-colors"
            :class="
              intervalSecs === preset.value
                ? 'border-blue-500 bg-blue-500/10 text-highlighted'
                : 'border-default hover:bg-elevated'
            "
            @click="intervalSecs = preset.value"
          >
            <span>{{ preset.label }}</span>
          </button>
        </div>

        <div class="flex justify-end gap-2 pt-1">
          <UButton variant="ghost" size="sm" @click="step = 'pick-preset'">Back</UButton>
          <UButton size="sm" :loading="submitting" @click="createSchedule">
            Create Schedule
          </UButton>
        </div>
      </div>
    </template>
  </UModal>
</template>
