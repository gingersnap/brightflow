<script setup lang="ts">
/**
 * Saved: every view of the source, grouped by table, each with what it does
 * in a line, who saved it and when. Open goes to Explore with the view
 * applied through the route; rename and delete go through the action bus
 * like the edits in Explore, so they are logged and undoable.
 */

import { computed } from 'vue';
import { useRouter } from 'vue-router';

import { usePromptAction } from '@/composables/usePromptAction';
import { useSavedViews } from '@/composables/useSavedViews';
import type { SavedViewResponse } from '@/types/generated';
import { describeExploreView, parseExploreView } from '@/utils/viewSpec';

const props = defineProps<{ sourceId: string }>();

const router = useRouter();
const promptText = usePromptAction();
const savedViews = useSavedViews(() => props.sourceId);

const byTable = computed(() => {
  const groups = new Map<string, SavedViewResponse[]>();
  for (const view of savedViews.views.value) {
    const list = groups.get(view.table) ?? [];
    list.push(view);
    groups.set(view.table, list);
  }
  return [...groups.entries()].map(([table, views]) => ({ table, views }));
});

function summary(view: SavedViewResponse): string {
  const spec = parseExploreView(view.spec);
  return spec == null ? 'saved by a newer version' : describeExploreView(spec);
}

function author(view: SavedViewResponse): string {
  const by = view.createdBy ?? '';
  if (by.startsWith('user:')) {
    return 'a person';
  }
  if (by.startsWith('agent:')) {
    return 'an agent run';
  }
  return 'unknown';
}

function formatTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

function open(view: SavedViewResponse): void {
  router.push({
    name: 'explore-table',
    params: { sourceId: props.sourceId, table: view.table },
    query: { view: view.id },
  });
}

async function rename(view: SavedViewResponse): Promise<void> {
  const name = await promptText('Rename view', {
    confirmLabel: 'Rename',
    description: view.name,
    initialValue: view.name,
    placeholder: 'A name for this view',
  });
  if (name != null && name !== view.name) {
    await savedViews.rename(view, name);
  }
}
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-4">
    <div class="mb-4">
      <h2 class="text-sm font-semibold text-highlighted">Saved views</h2>
      <p class="text-sm text-muted">
        Explore configurations saved for this source. Open one to apply it; the builder is yours
        from there.
      </p>
    </div>

    <p v-if="byTable.length === 0" class="text-sm text-muted">
      Nothing saved yet. In Explore, set up filters or a pivot and choose "Save view".
    </p>

    <section v-for="group in byTable" :key="group.table" class="mb-6">
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
        {{ group.table }}
      </h3>
      <ul class="divide-y divide-default rounded-lg border border-default bg-elevated">
        <li
          v-for="view in group.views"
          :key="view.id"
          class="flex flex-wrap items-center justify-between gap-3 px-4 py-3"
        >
          <div class="min-w-0">
            <p class="text-sm font-medium text-highlighted">{{ view.name }}</p>
            <p class="text-sm text-muted">
              {{ summary(view) }} · saved by {{ author(view) }} · {{ formatTime(view.updatedAt) }}
            </p>
          </div>
          <div class="flex flex-shrink-0 items-center gap-1">
            <UButton
              size="md"
              color="primary"
              variant="soft"
              icon="i-lucide-search"
              @click="open(view)"
            >
              Open
            </UButton>
            <UButton
              size="md"
              color="neutral"
              variant="ghost"
              icon="i-lucide-pencil-line"
              :aria-label="`Rename ${view.name}`"
              @click="() => void rename(view)"
            />
            <UButton
              size="md"
              color="neutral"
              variant="ghost"
              icon="i-lucide-trash-2"
              :aria-label="`Delete ${view.name}`"
              @click="() => void savedViews.remove(view)"
            />
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>
