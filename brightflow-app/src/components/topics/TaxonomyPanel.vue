<script setup lang="ts">
import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';

import { useCuration } from '@/composables/useCuration';
import { taxonomyApi } from '@/services/api';
import type { TaxonomyCategory } from '@/types/generated';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const curation = useCuration();
const queryCache = useQueryCache();

const { data: taxonomy, isLoading } = useQuery({
  key: () => ['taxonomy', props.sourceId, props.table],
  query: () => taxonomyApi.overview(props.sourceId, props.table),
});

async function refresh(): Promise<void> {
  await queryCache.invalidateQueries({ key: ['taxonomy', props.sourceId, props.table] });
}

const categories = computed(() => taxonomy.value?.categories ?? []);

/** Categories the classifier would drop at fit time — where curation time pays off most. */
const underSupported = computed(() => categories.value.filter((c) => !c.trainable));

const trainingReady = computed(() => {
  const t = taxonomy.value;
  if (t == null) {
    return false;
  }
  return t.labelledRows >= t.minTrainRows && categories.value.some((c) => c.trainable);
});

const busy = ref(false);

async function defineCategory(): Promise<void> {
  const name = window.prompt('New intent category (what is WRONG, not how it is written)');
  if (name == null || name.trim() === '') {
    return;
  }
  const description = window.prompt('One-sentence description of the symptom (optional)');
  busy.value = true;
  try {
    await curation.dispatch({
      kind: 'define_taxonomy_category',
      source_id: props.sourceId,
      table: props.table,
      name: name.trim(),
      description: description != null && description.trim() !== '' ? description.trim() : null,
    });
    await refresh();
  } finally {
    busy.value = false;
  }
}

async function renameCategory(category: TaxonomyCategory): Promise<void> {
  const name = window.prompt('Rename category', category.name);
  if (name == null || name.trim() === '' || name.trim() === category.name) {
    return;
  }
  busy.value = true;
  try {
    await curation.dispatch({
      kind: 'rename_taxonomy_category',
      source_id: props.sourceId,
      table: props.table,
      category_id: category.id,
      name: name.trim(),
    });
    await refresh();
  } finally {
    busy.value = false;
  }
}

async function deleteCategory(category: TaxonomyCategory): Promise<void> {
  const confirmed = window.confirm(
    `Delete "${category.name}"? Its ${category.labelledRows} row label(s) go too. This is undoable from the activity feed.`,
  );
  if (!confirmed) {
    return;
  }
  busy.value = true;
  try {
    await curation.dispatch({
      kind: 'delete_taxonomy_category',
      source_id: props.sourceId,
      table: props.table,
      category_id: category.id,
    });
    await refresh();
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex items-start justify-between gap-3">
      <div class="flex flex-col gap-1">
        <h3 class="text-sm font-medium text-highlighted">Intent taxonomy</h3>
        <p class="text-sm text-muted">
          What tickets are <em>about</em>. Clusters below group by writing format; these categories
          are the answer the classifier learns to predict.
        </p>
      </div>
      <UButton
        size="md"
        color="neutral"
        variant="outline"
        icon="i-lucide-plus"
        :loading="busy"
        @click="() => void defineCategory()"
      >
        Add category
      </UButton>
    </div>

    <div v-if="isLoading" class="flex items-center gap-2 text-sm text-muted">
      <UIcon name="i-lucide-loader-circle" class="animate-spin" />
      Loading taxonomy…
    </div>

    <div v-else-if="categories.length === 0" class="rounded-lg border border-default p-4">
      <p class="text-sm text-muted">
        No categories yet. Run the <strong>propose_taxonomy</strong> agent to draft a vocabulary,
        then approve its proposals — or add one by hand.
      </p>
    </div>

    <template v-else>
      <!-- Coverage: the numbers that decide whether a fit will produce anything. -->
      <div class="grid grid-cols-3 gap-3">
        <div class="rounded-lg border border-default p-3">
          <div class="text-sm text-muted">Labelled rows</div>
          <div class="text-base font-medium text-highlighted">
            {{ taxonomy?.labelledRows.toLocaleString() ?? '—' }}
            <span class="text-sm text-muted">/ {{ taxonomy?.totalRows.toLocaleString() }}</span>
          </div>
        </div>
        <div class="rounded-lg border border-default p-3">
          <div class="text-sm text-muted">Categories</div>
          <div class="text-base font-medium text-highlighted">
            {{ categories.filter((c) => c.trainable).length }}
            <span class="text-sm text-muted">of {{ categories.length }} trainable</span>
          </div>
        </div>
        <div class="rounded-lg border border-default p-3">
          <div class="text-sm text-muted">Classifier macro-F1</div>
          <div class="text-base font-medium text-highlighted">
            {{ taxonomy?.classifierValMacroF1?.toFixed(3) ?? '—' }}
          </div>
        </div>
      </div>

      <div
        v-if="!trainingReady"
        class="rounded-lg border border-warning bg-warning/10 p-3 text-sm text-default"
      >
        Not enough curated labels to train yet — need at least
        {{ taxonomy?.minTrainRows }} labelled rows and one category with
        {{ taxonomy?.minLabelSupport }}+ examples.
      </div>

      <div
        v-else-if="underSupported.length > 0"
        class="rounded-lg border border-default bg-elevated p-3 text-sm text-muted"
      >
        {{ underSupported.length }} categor{{ underSupported.length === 1 ? 'y' : 'ies' }} will be
        dropped at fit time (under {{ taxonomy?.minLabelSupport }} examples):
        <span class="text-default">{{ underSupported.map((c) => c.name).join(', ') }}</span>
      </div>

      <ul class="flex flex-col gap-2">
        <li
          v-for="category in categories"
          :key="category.id"
          class="flex items-start justify-between gap-3 rounded-lg border border-default bg-elevated p-3"
        >
          <div class="flex min-w-0 flex-col gap-1">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-highlighted">{{ category.name }}</span>
              <UBadge
                size="lg"
                :color="category.trainable ? 'success' : 'warning'"
                variant="subtle"
              >
                {{ category.labelledRows }}
              </UBadge>
            </div>
            <p v-if="category.description" class="line-clamp-2 text-sm text-muted">
              {{ category.description }}
            </p>
          </div>
          <div class="flex shrink-0 gap-1">
            <UButton
              size="md"
              color="neutral"
              variant="ghost"
              icon="i-lucide-pencil"
              aria-label="Rename category"
              :disabled="busy"
              @click="() => void renameCategory(category)"
            />
            <UButton
              size="md"
              color="neutral"
              variant="ghost"
              icon="i-lucide-trash-2"
              aria-label="Delete category"
              :disabled="busy"
              @click="() => void deleteCategory(category)"
            />
          </div>
        </li>
      </ul>
    </template>
  </div>
</template>
