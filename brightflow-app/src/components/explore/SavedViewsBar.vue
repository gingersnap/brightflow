<script setup lang="ts">
/**
 * The saved-views control in Explore: a menu of the table's views to apply,
 * "Save as new view…", and — while a view is applied — update, rename and
 * delete for it. Applying writes the spec into the query and pivot stores,
 * whose watchers re-run the queries; nothing here executes a query. The
 * applied view's id is the parent's, so the route can carry it.
 */

import type { DropdownMenuItem } from '@nuxt/ui';
import { computed } from 'vue';

import { usePromptAction } from '@/composables/usePromptAction';
import { useSavedViews } from '@/composables/useSavedViews';
import { usePivotStore } from '@/stores/pivot';
import { useQueryStore } from '@/stores/query';
import type { SavedViewResponse } from '@/types/generated';
import { applyExploreView, captureExploreView, parseExploreView } from '@/utils/viewSpec';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

/** The applied view, or null when the builder holds unsaved state. */
const activeViewId = defineModel<string | null>('activeViewId', { default: null });

const queryStore = useQueryStore();
const pivotStore = usePivotStore();
const promptText = usePromptAction();
const savedViews = useSavedViews(() => props.sourceId);

const views = computed(() => savedViews.forTable(props.table));
const active = computed(() => views.value.find((v) => v.id === activeViewId.value) ?? null);

function apply(view: SavedViewResponse): void {
  const spec = parseExploreView(view.spec);
  if (spec == null) {
    return;
  }
  applyExploreView(spec, queryStore, pivotStore);
  activeViewId.value = view.id;
}

async function saveAsNew(): Promise<void> {
  const name = await promptText('Save view', {
    confirmLabel: 'Save',
    description: props.table,
    placeholder: 'A name for this view',
  });
  if (name == null) {
    return;
  }
  const spec = captureExploreView(queryStore, pivotStore);
  if (await savedViews.save({ name, spec, table: props.table })) {
    activeViewId.value = views.value.find((v) => v.name === name)?.id ?? null;
  }
}

async function update(): Promise<void> {
  const view = active.value;
  if (view == null) {
    return;
  }
  await savedViews.save({
    name: view.name,
    spec: captureExploreView(queryStore, pivotStore),
    table: props.table,
    viewId: view.id,
  });
}

async function rename(): Promise<void> {
  const view = active.value;
  if (view == null) {
    return;
  }
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

async function remove(): Promise<void> {
  const view = active.value;
  if (view == null) {
    return;
  }
  await savedViews.remove(view);
  activeViewId.value = null;
}

const menu = computed<DropdownMenuItem[][]>(() => {
  const list: DropdownMenuItem[] = views.value.map((view) => ({
    checked: view.id === activeViewId.value,
    label: view.name,
    onSelect: () => apply(view),
    type: 'checkbox',
  }));
  const own: DropdownMenuItem[] =
    active.value == null
      ? []
      : [
          {
            icon: 'i-lucide-save',
            label: `Update "${active.value.name}"`,
            onSelect: () => void update(),
          },
          { icon: 'i-lucide-pencil-line', label: 'Rename…', onSelect: () => void rename() },
          { icon: 'i-lucide-trash-2', label: 'Delete', onSelect: () => void remove() },
        ];
  const groups: DropdownMenuItem[][] = [];
  if (list.length > 0) {
    groups.push(list);
  }
  groups.push([
    {
      icon: 'i-lucide-bookmark-plus',
      label: 'Save as new view…',
      onSelect: () => void saveAsNew(),
    },
  ]);
  if (own.length > 0) {
    groups.push(own);
  }
  return groups;
});
</script>

<template>
  <UDropdownMenu :items="menu">
    <UButton
      size="md"
      color="neutral"
      variant="soft"
      :icon="active ? 'i-lucide-bookmark-check' : 'i-lucide-bookmark'"
    >
      {{ active ? active.name : views.length > 0 ? `Views (${views.length})` : 'Save view' }}
    </UButton>
  </UDropdownMenu>
</template>
