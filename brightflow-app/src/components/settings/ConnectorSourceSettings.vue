<script setup lang="ts">
/**
 * Settings page for a connector-backed source. Name and token are editable
 * in place (the token write-only — only a has-token flag is shown), while the
 * connector type and its config are displayed read-only: connector settings
 * are fixed for the lifetime of a source, and syncing a different target
 * means creating a new one. Deletion keeps stored data.
 */

import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import { hintsFor } from '@/components/connect/connectorHints';
import SchedulesPanel from '@/components/connect/SchedulesPanel.vue';
import { connectApi } from '@/services/api';
import type { UnifiedSource } from '@/types/generated';

const props = defineProps<{
  source: UnifiedSource;
}>();

const router = useRouter();
const queryCache = useQueryCache();

const presetId = computed(() => props.source.id.replace(/^connector:/u, ''));
const configKey = computed(() => ['connector-config', presetId.value]);

const { data: config, refresh: refreshConfig } = useQuery({
  key: () => configKey.value,
  query: async () => await connectApi.getConfig(presetId.value),
});

function invalidateCommon(): void {
  queryCache.invalidateQueries({ key: ['unified-sources'] });
  queryCache.invalidateQueries({ key: ['connectors'] });
  void refreshConfig();
}

// --- Name editing ---
const editingName = ref(false);
const nameDraft = ref('');
const nameError = ref<string | null>(null);

function startEditName(): void {
  nameDraft.value = props.source.name;
  nameError.value = null;
  editingName.value = true;
}

function cancelEditName(): void {
  editingName.value = false;
  nameError.value = null;
}

const { mutate: saveName, isLoading: savingName } = useMutation({
  mutation: async (name: string) => await connectApi.updateConfig(presetId.value, { name }),
  onSuccess: () => {
    editingName.value = false;
    invalidateCommon();
  },
  onError: (error: unknown) => {
    nameError.value = error instanceof Error ? error.message : 'Failed to save name';
  },
});

function submitName(): void {
  const trimmed = nameDraft.value.trim();
  if (!trimmed) {
    return;
  }
  saveName(trimmed);
}

// --- Token editing ---
const editingToken = ref(false);
const tokenDraft = ref('');
const tokenError = ref<string | null>(null);
const showToken = ref(false);

function toggleTokenEdit(): void {
  editingToken.value = !editingToken.value;
  tokenDraft.value = '';
  tokenError.value = null;
  showToken.value = false;
}

const { mutate: saveToken, isLoading: savingToken } = useMutation({
  mutation: async (token: string) => await connectApi.updateConfig(presetId.value, { token }),
  onSuccess: () => {
    editingToken.value = false;
    tokenDraft.value = '';
    invalidateCommon();
  },
  onError: (error: unknown) => {
    tokenError.value = error instanceof Error ? error.message : 'Failed to save token';
  },
});

function submitToken(): void {
  if (!tokenDraft.value.trim()) {
    return;
  }
  saveToken(tokenDraft.value.trim());
}

// --- Connector config (read-only display) ---
const connectorPath = computed(
  () => config.value?.connectorPath ?? props.source.connectorName ?? '',
);

const parsedConfig = computed<Record<string, string>>(() => {
  const raw = config.value?.configJson ?? '{}';
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      const out: Record<string, string> = {};
      for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
        out[k] = typeof v === 'string' ? v : String(v);
      }
      return out;
    }
  } catch {
    // Fall through
  }
  return {};
});

const configRows = computed<{ key: string; label: string; value: string }[]>(() => {
  const entries = parsedConfig.value;
  const hints = hintsFor(connectorPath.value);
  const hintKeys = new Set(hints.map((h) => h.key));
  const rows: { key: string; label: string; value: string }[] = [];
  for (const hint of hints) {
    rows.push({ key: hint.key, label: hint.label, value: entries[hint.key] ?? '' });
  }
  for (const [k, v] of Object.entries(entries)) {
    if (!hintKeys.has(k)) {
      rows.push({ key: k, label: k, value: v });
    }
  }
  return rows;
});

// --- Delete ---
const confirmingDelete = ref(false);
const deleteError = ref<string | null>(null);

const { mutate: deleteSource, isLoading: deleting } = useMutation({
  mutation: async () => await connectApi.deleteConfig(presetId.value),
  onSuccess: async () => {
    queryCache.invalidateQueries({ key: ['unified-sources'] });
    queryCache.invalidateQueries({ key: ['connectors'] });
    await router.push({ name: 'sources' });
  },
  onError: (error: unknown) => {
    deleteError.value = error instanceof Error ? error.message : 'Failed to delete source';
  },
});

watch(presetId, () => void refreshConfig());
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-6">
    <div class="mx-auto w-full max-w-3xl space-y-6">
      <!-- Name -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Name</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <div v-if="!editingName" class="flex items-center justify-between">
            <div>
              <p class="text-sm text-highlighted">{{ source.name }}</p>
              <p v-if="connectorPath" class="mt-0.5 text-sm text-muted">{{ connectorPath }}</p>
            </div>
            <UButton variant="ghost" size="md" @click="startEditName">
              <UIcon name="i-lucide-pencil" class="mr-1 h-3.5 w-3.5" />
              Edit
            </UButton>
          </div>
          <InlineRename
            v-else
            v-model="nameDraft"
            :saving="savingName"
            :error="nameError"
            @save="submitName"
            @cancel="cancelEditName"
          />
        </div>
      </section>

      <!-- Token -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Token</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <div class="flex items-center justify-between">
            <span
              class="inline-flex items-center gap-1.5 text-sm"
              :class="config?.hasToken ? 'text-green-500' : 'text-amber-500'"
            >
              <UIcon name="i-lucide-key" class="h-3.5 w-3.5" />
              {{ config?.hasToken ? 'Token set' : 'No token' }}
            </span>
            <UButton variant="ghost" size="md" @click="toggleTokenEdit">
              {{ editingToken ? 'Cancel' : config?.hasToken ? 'Update' : 'Add token' }}
            </UButton>
          </div>
          <div v-if="editingToken" class="mt-3 space-y-2">
            <UInput
              v-model="tokenDraft"
              :type="showToken ? 'text' : 'password'"
              name="brightflow-source-token"
              autocomplete="new-password"
              data-1p-ignore
              data-lpignore="true"
              placeholder="Paste API token..."
              class="w-full"
              :ui="{ trailing: 'pe-1' }"
              @keyup.enter="submitToken"
            >
              <template #trailing>
                <UButton
                  color="neutral"
                  variant="link"
                  size="xs"
                  :icon="showToken ? 'i-lucide-eye-off' : 'i-lucide-eye'"
                  :aria-label="showToken ? 'Hide token' : 'Show token'"
                  @click="showToken = !showToken"
                />
              </template>
            </UInput>
            <div v-if="tokenError" class="text-sm text-red-500">{{ tokenError }}</div>
            <div class="flex justify-end">
              <UButton
                size="md"
                :loading="savingToken"
                :disabled="!tokenDraft.trim()"
                @click="submitToken"
              >
                Save token
              </UButton>
            </div>
          </div>
        </div>
      </section>

      <!-- Connector config (read-only) -->
      <section v-if="config">
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Connector</h3>
        <div class="rounded-lg border border-default bg-elevated p-4">
          <dl class="space-y-2 text-sm">
            <div class="flex items-baseline gap-3">
              <dt class="w-24 shrink-0 text-sm text-muted">Type</dt>
              <dd class="text-highlighted">{{ connectorPath }}</dd>
            </div>
            <div v-for="row in configRows" :key="row.key" class="flex items-baseline gap-3">
              <dt class="w-24 shrink-0 text-sm text-muted">{{ row.label }}</dt>
              <dd class="break-all text-highlighted">
                <template v-if="row.value">{{ row.value }}</template>
                <span v-else class="text-muted italic">not set</span>
              </dd>
            </div>
          </dl>
          <p class="mt-3 text-sm text-muted">
            Connector settings are fixed for the lifetime of a source. To sync a different target,
            add a new source.
          </p>
        </div>
      </section>

      <!-- Schedules + runs for this preset -->
      <SchedulesPanel :preset-name="source.name" />

      <!-- Danger zone -->
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Danger zone</h3>
        <div class="space-y-2 rounded-lg border border-default bg-elevated p-4">
          <div v-if="deleteError" class="text-sm text-red-500">{{ deleteError }}</div>
          <div v-if="!confirmingDelete">
            <UButton variant="ghost" color="error" size="md" @click="confirmingDelete = true">
              <UIcon name="i-lucide-trash-2" class="mr-1.5 h-3.5 w-3.5" />
              Delete source
            </UButton>
          </div>
          <div v-else class="space-y-2">
            <p class="text-sm text-muted">
              This will remove the preset and its schedules. Stored data is kept.
            </p>
            <div class="flex gap-2">
              <UButton variant="ghost" size="md" @click="confirmingDelete = false">Cancel</UButton>
              <UButton color="error" size="md" :loading="deleting" @click="deleteSource()">
                Yes, delete
              </UButton>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
