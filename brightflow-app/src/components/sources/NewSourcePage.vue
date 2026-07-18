<script setup lang="ts">
import { Database, Globe, Upload } from '@lucide/vue';
import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';

import PresetForm from '@/components/connect/PresetForm.vue';
import UploadCsvForm from '@/components/sources/UploadCsvForm.vue';
import { connectApi, sourceApi } from '@/services/api';
import type { AvailableConnectorResponse } from '@/types';

const router = useRouter();
const queryCache = useQueryCache();

type Tab = 'web' | 'connector' | 'upload';
const tab = ref<Tab>('web');

// --- Web-analytics form ---
const domain = ref('');
const webName = ref('');
const webSubmitting = ref(false);
const webError = ref<string | null>(null);

async function submitWeb(): Promise<void> {
  const trimmed = domain.value.trim();
  if (!trimmed) {
    return;
  }
  webSubmitting.value = true;
  webError.value = null;
  try {
    const source = await sourceApi.create(trimmed, webName.value.trim() || trimmed);
    if (!source) {
      throw new Error('Failed to create source');
    }
    queryCache.invalidateQueries({ key: ['unified-sources'] });
    await router.push({
      name: 'source-tool',
      params: { sourceId: `web:${source.id}`, tool: 'settings' },
    });
  } catch (error) {
    webError.value = error instanceof Error ? error.message : 'Failed to create source';
  } finally {
    webSubmitting.value = false;
  }
}

// --- Connector picker ---
const { data: available } = useQuery({
  key: ['connectors-available'],
  query: async () => {
    const result = await connectApi.listAvailable();
    return result ?? ([] as AvailableConnectorResponse[]);
  },
});

const availableList = computed(() => available.value ?? []);
const selectedConnector = ref<AvailableConnectorResponse | null>(null);
const connectorError = ref<string | null>(null);
const connectorSubmitting = ref(false);

function pickConnector(c: AvailableConnectorResponse): void {
  selectedConnector.value = c;
  connectorError.value = null;
}

function backToConnectorList(): void {
  selectedConnector.value = null;
  connectorError.value = null;
}

async function handlePresetCreate(data: {
  name: string;
  token: string;
  config: Record<string, string>;
}): Promise<void> {
  const connector = selectedConnector.value;
  if (!connector) {
    return;
  }
  connectorSubmitting.value = true;
  connectorError.value = null;
  try {
    const preset = await connectApi.createPreset({
      name: data.name || `${connector.name}-default`,
      connectorPath: connector.name,
      configJson: data.config,
      token: data.token || undefined,
    });
    if (!preset?.id) {
      throw new Error('Failed to create preset');
    }
    queryCache.invalidateQueries({ key: ['unified-sources'] });
    queryCache.invalidateQueries({ key: ['connectors'] });
    queryCache.invalidateQueries({ key: ['connectors-available'] });
    await router.push({
      name: 'source-tool',
      params: { sourceId: `connector:${preset.id}`, tool: 'settings' },
    });
  } catch (error) {
    connectorError.value = error instanceof Error ? error.message : 'Failed to create preset';
  } finally {
    connectorSubmitting.value = false;
  }
}
</script>

<template>
  <UDashboardPanel id="sources-new">
    <template #header>
      <UDashboardNavbar title="Add source">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="p-4">
        <div class="mx-auto max-w-2xl space-y-6">
          <!-- Tab selector -->
          <div class="inline-flex rounded-lg border border-default bg-default p-1">
            <button
              class="flex cursor-pointer items-center gap-2 rounded px-3 py-1.5 text-sm transition-colors"
              :class="
                tab === 'web' ? 'bg-elevated text-highlighted' : 'text-muted hover:text-highlighted'
              "
              @click="tab = 'web'"
            >
              <Globe class="h-4 w-4" />
              Web analytics
            </button>
            <button
              class="flex cursor-pointer items-center gap-2 rounded px-3 py-1.5 text-sm transition-colors"
              :class="
                tab === 'connector'
                  ? 'bg-elevated text-highlighted'
                  : 'text-muted hover:text-highlighted'
              "
              @click="tab = 'connector'"
            >
              <Database class="h-4 w-4" />
              Connector
            </button>
            <button
              class="flex cursor-pointer items-center gap-2 rounded px-3 py-1.5 text-sm transition-colors"
              :class="
                tab === 'upload'
                  ? 'bg-elevated text-highlighted'
                  : 'text-muted hover:text-highlighted'
              "
              @click="tab = 'upload'"
            >
              <Upload class="h-4 w-4" />
              CSV upload
            </button>
          </div>

          <!-- Web tab -->
          <section
            v-if="tab === 'web'"
            class="space-y-4 rounded-lg border border-default bg-elevated p-4"
          >
            <div v-if="webError" class="rounded bg-red-500/10 p-2 text-sm text-red-500">
              {{ webError }}
            </div>
            <form class="space-y-3" @submit.prevent="submitWeb">
              <div>
                <label class="mb-1 block text-sm font-medium text-muted">Domain</label>
                <input
                  v-model="domain"
                  type="text"
                  placeholder="example.com"
                  required
                  class="placeholder-muted w-full rounded border border-default bg-default px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
                />
              </div>
              <div>
                <label class="mb-1 block text-sm font-medium text-muted">
                  Name <span class="text-muted">(optional)</span>
                </label>
                <input
                  v-model="webName"
                  type="text"
                  :placeholder="domain || 'My site'"
                  class="placeholder-muted w-full rounded border border-default bg-default px-2.5 py-1.5 text-sm text-highlighted focus:border-blue-500 focus:outline-none"
                />
              </div>
              <div class="flex justify-end pt-1">
                <UButton
                  type="submit"
                  size="md"
                  :loading="webSubmitting"
                  :disabled="!domain.trim()"
                >
                  Add web source
                </UButton>
              </div>
            </form>
          </section>

          <!-- Upload tab -->
          <UploadCsvForm v-else-if="tab === 'upload'" />

          <!-- Connector tab -->
          <section v-else class="space-y-4">
            <div v-if="connectorError" class="rounded bg-red-500/10 p-2 text-sm text-red-500">
              {{ connectorError }}
            </div>

            <!-- Picker -->
            <div v-if="!selectedConnector" class="space-y-2">
              <button
                v-for="c in availableList"
                :key="c.name"
                class="flex w-full cursor-pointer items-center gap-3 rounded-lg border border-default px-3 py-2.5 text-left transition-colors hover:bg-elevated"
                @click="pickConnector(c)"
              >
                <Database class="h-5 w-5 shrink-0 text-muted" />
                <div class="min-w-0 flex-1">
                  <div class="flex items-baseline gap-2">
                    <span class="text-sm font-medium text-highlighted">{{ c.name }}</span>
                    <span v-if="c.version" class="text-xs text-muted">v{{ c.version }}</span>
                  </div>
                  <p v-if="c.description" class="mt-0.5 text-sm text-muted">{{ c.description }}</p>
                </div>
              </button>

              <p v-if="availableList.length === 0" class="py-4 text-center text-sm text-muted">
                No connectors available
              </p>
            </div>

            <!-- Preset form for the picked connector -->
            <div v-else class="space-y-3 rounded-lg border border-default bg-elevated p-4">
              <div class="flex items-baseline justify-between">
                <h3 class="text-sm font-semibold text-highlighted">
                  New {{ selectedConnector.name }} source
                </h3>
                <button
                  class="cursor-pointer text-sm text-muted hover:text-highlighted"
                  @click="backToConnectorList"
                >
                  Change connector
                </button>
              </div>
              <PresetForm
                :connector-name="selectedConnector.name"
                mode="create"
                @submit="handlePresetCreate"
                @cancel="backToConnectorList"
              />
              <p v-if="connectorSubmitting" class="text-sm text-muted">Creating preset…</p>
            </div>
          </section>
        </div>
      </div>
    </template>
  </UDashboardPanel>
</template>
