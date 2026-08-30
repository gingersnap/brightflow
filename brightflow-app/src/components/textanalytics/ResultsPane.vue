<script setup lang="ts">
/**
 * Results pane: the two grains side by side. Ticket grain answers
 * "billing tickets up" for operations; mention grain answers "invoice screen
 * mentioned 340 times, 78 % negative, 61 % incidental" for product. Below
 * them, the unresolved-subject queue (what the extractor named that the
 * catalog does not know) and per-level vocabulary health.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref } from 'vue';

import { enrichFnApi, mentionsApi, taxonomyApi, ticketsApi, vocabularyApi } from '@/services/api';
import type { UnresolvedSubjectResponse } from '@/types/generated';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const queryCache = useQueryCache();

const { data: tickets } = useQuery({
  key: () => ['tickets-summary', props.sourceId, props.table],
  query: () => ticketsApi.summary(props.sourceId, props.table),
});
const { data: mentions } = useQuery({
  key: () => ['mentions-summary', props.sourceId, props.table],
  query: () => mentionsApi.summary(props.sourceId, props.table),
});
const { data: health } = useQuery({
  key: () => ['vocabulary-health', props.sourceId, props.table],
  query: () => vocabularyApi.health(props.sourceId, props.table),
});
const unresolvedKey = computed(() => ['unresolved', props.sourceId, props.table]);
const { data: unresolved } = useQuery({
  key: () => unresolvedKey.value,
  query: async () => (await vocabularyApi.unresolved(props.sourceId, props.table)) ?? [],
});
const { data: taxonomy } = useQuery({
  key: () => ['taxonomy', props.sourceId, props.table],
  query: () => taxonomyApi.overview(props.sourceId, props.table),
});
const { data: functions } = useQuery({
  key: () => ['enrich-fns', props.sourceId, props.table],
  query: async () => (await enrichFnApi.list(props.sourceId, props.table)) ?? [],
});

const pct = (share: number): string => `${Math.round(share * 100)}%`;

/** Products and competitors — the only kinds a subject can map onto. */
const mapTargets = computed(() =>
  (taxonomy.value?.categories ?? [])
    .filter((c) => c.kind === 'product' || c.kind === 'competitor')
    .map((c) => ({ label: `${c.name} (${c.kind})`, value: c.id })),
);

/** Chosen map target per queue row; absent until the user picks one. */
const mapping = ref<Record<number, number>>({});
const busy = ref(false);

async function resolve(
  row: UnresolvedSubjectResponse,
  status: 'mapped' | 'ignored',
): Promise<void> {
  const mappedTo = mapping.value[row.id];
  if (status === 'mapped' && mappedTo == null) {
    return;
  }
  busy.value = true;
  try {
    await vocabularyApi.updateUnresolved(props.sourceId, props.table, {
      id: row.id,
      status,
      ...(mappedTo == null ? {} : { mappedTo }),
    });
    await queryCache.invalidateQueries({ key: unresolvedKey.value });
  } finally {
    busy.value = false;
  }
}

/** Mapped subjects reach the table on the next materialise — no LLM call. */
async function applyMappings(): Promise<void> {
  const extract = (functions.value ?? []).find((f) => f.kind === 'ticket_extract');
  if (extract == null) {
    return;
  }
  busy.value = true;
  try {
    await enrichFnApi.materialize(extract.id);
    await Promise.all([
      queryCache.invalidateQueries({ key: ['mentions-summary', props.sourceId, props.table] }),
      queryCache.invalidateQueries({ key: unresolvedKey.value }),
    ]);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="flex flex-col gap-8">
    <div class="grid grid-cols-1 gap-6 xl:grid-cols-2">
      <!-- Ticket grain -->
      <section class="flex flex-col gap-3">
        <div>
          <h3 class="text-sm font-medium text-highlighted">Tickets</h3>
          <p class="text-sm text-muted">
            {{ tickets?.classifiedRows.toLocaleString() ?? 0 }} of
            {{ tickets?.totalRows.toLocaleString() ?? 0 }} classified
          </p>
        </div>
        <p v-if="(tickets?.categories.length ?? 0) === 0" class="text-sm text-muted">
          Nothing classified yet — set up and run classification first.
        </p>
        <ul v-else class="flex flex-col gap-2">
          <li
            v-for="cat in tickets?.categories ?? []"
            :key="cat.category"
            class="rounded-lg border border-default bg-elevated p-3"
          >
            <div class="flex items-center justify-between">
              <span class="text-sm font-medium text-highlighted">{{ cat.category }}</span>
              <span class="text-sm text-muted">{{ cat.rows.toLocaleString() }}</span>
            </div>
            <ul v-if="cat.subcategories.length > 0" class="mt-1 flex flex-wrap gap-x-3 gap-y-1">
              <li v-for="sub in cat.subcategories" :key="sub.value" class="text-sm text-muted">
                {{ sub.value }} <span class="text-dimmed">{{ sub.rows }}</span>
              </li>
            </ul>
          </li>
        </ul>
        <div v-if="tickets" class="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
          <span v-for="s in tickets.sentiment" :key="s.value">{{ s.value }} {{ s.rows }}</span>
          <span class="text-dimmed">·</span>
          <span v-for="l in tickets.languages" :key="l.value">{{ l.value }} {{ l.rows }}</span>
        </div>
      </section>

      <!-- Mention grain -->
      <section class="flex flex-col gap-3">
        <div>
          <h3 class="text-sm font-medium text-highlighted">Mentions</h3>
          <p class="text-sm text-muted">
            {{ mentions?.totalMentions.toLocaleString() ?? 0 }} mentions · ranked by distinct
            tickets, the honest count
          </p>
        </div>
        <p v-if="(mentions?.subjects.length ?? 0) === 0" class="text-sm text-muted">
          No mentions yet — set up and run extraction first.
        </p>
        <div v-else class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-default text-left text-muted">
                <th class="px-2 py-1.5 font-medium">subject</th>
                <th class="px-2 py-1.5 font-medium">type</th>
                <th class="px-2 py-1.5 text-right font-medium">tickets</th>
                <th class="px-2 py-1.5 text-right font-medium">mentions</th>
                <th class="px-2 py-1.5 text-right font-medium">negative</th>
                <th class="px-2 py-1.5 text-right font-medium">incidental</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="s in mentions?.subjects ?? []"
                :key="`${s.mentionType}:${s.subject}`"
                class="border-b border-default/50"
              >
                <td class="px-2 py-1.5 text-highlighted">
                  {{ s.subject }}
                  <UBadge v-if="!s.resolved" size="sm" color="warning" variant="subtle">
                    unresolved
                  </UBadge>
                </td>
                <td class="px-2 py-1.5 text-muted">{{ s.mentionType }}</td>
                <td class="px-2 py-1.5 text-right">{{ s.distinctTickets }}</td>
                <td class="px-2 py-1.5 text-right text-muted">{{ s.mentions }}</td>
                <td class="px-2 py-1.5 text-right">{{ pct(s.negativeShare) }}</td>
                <td class="px-2 py-1.5 text-right">{{ pct(s.incidentalShare) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </div>

    <!-- Unresolved subjects -->
    <section class="flex flex-col gap-3">
      <div class="flex items-start justify-between gap-3">
        <div>
          <h3 class="text-sm font-medium text-highlighted">Unresolved subjects</h3>
          <p class="text-sm text-muted">
            Names the extractor used that match no product or competitor. Map one to an entry (it
            becomes an alias) or ignore it; then apply so the mention table picks it up.
          </p>
        </div>
        <UButton
          size="md"
          color="neutral"
          variant="outline"
          icon="i-lucide-refresh-cw"
          :loading="busy"
          @click="() => void applyMappings()"
        >
          Apply mappings
        </UButton>
      </div>
      <p v-if="(unresolved?.length ?? 0) === 0" class="text-sm text-muted">Queue is empty.</p>
      <ul v-else class="flex flex-col gap-2">
        <li
          v-for="row in unresolved ?? []"
          :key="row.id"
          class="flex flex-wrap items-center gap-3 rounded-lg border border-default bg-elevated p-3"
        >
          <span class="text-sm font-medium text-highlighted">{{ row.surface }}</span>
          <UBadge size="sm" color="neutral" variant="subtle">{{ row.kind }}</UBadge>
          <span class="text-sm text-muted">{{ row.mentionCount }} mentions</span>
          <div class="ml-auto flex items-center gap-2">
            <!-- Conditional spread: an explicit `undefined` is not an allowed prop value. -->
            <USelectMenu
              v-bind="mapping[row.id] == null ? {} : { modelValue: mapping[row.id] }"
              :items="mapTargets"
              @update:model-value="(v: number) => (mapping[row.id] = v)"
              value-key="value"
              size="md"
              class="w-56"
              placeholder="Map to…"
            />
            <UButton
              size="md"
              color="primary"
              variant="soft"
              :disabled="busy || mapping[row.id] == null"
              @click="() => void resolve(row, 'mapped')"
            >
              Map
            </UButton>
            <UButton
              size="md"
              color="neutral"
              variant="ghost"
              :disabled="busy"
              @click="() => void resolve(row, 'ignored')"
            >
              Ignore
            </UButton>
          </div>
        </li>
      </ul>
    </section>

    <!-- Vocabulary health -->
    <section class="flex flex-col gap-3">
      <div>
        <h3 class="text-sm font-medium text-highlighted">Vocabulary health</h3>
        <p class="text-sm text-muted">
          Count is only a cold-start signal. Once there is data: an other-rate above 15 % means the
          vocabulary is wrong; nothing should sit above 40 % or below 2 %.
        </p>
      </div>
      <p v-if="(health?.levels.length ?? 0) === 0" class="text-sm text-muted">
        No classified rows yet.
      </p>
      <ul v-else class="flex flex-col gap-2">
        <li
          v-for="level in [
            ...(health?.levels ?? []).map((h) => ({ parent: null as string | null, health: h })),
            ...(health?.perParent ?? []),
          ]"
          :key="`${level.health.kind}:${level.parent ?? ''}`"
          class="rounded-lg border border-default bg-elevated p-3"
        >
          <div class="flex flex-wrap items-center gap-3">
            <span class="text-sm font-medium text-highlighted">
              {{ level.health.kind
              }}<template v-if="level.parent"> under {{ level.parent }}</template>
            </span>
            <span class="text-sm text-muted">
              {{ level.health.entries }} / {{ level.health.cap }} entries ·
              {{ level.health.rows.toLocaleString() }} rows
            </span>
            <UBadge
              size="sm"
              :color="level.health.otherRate > 0.15 ? 'warning' : 'neutral'"
              variant="subtle"
            >
              other {{ pct(level.health.otherRate) }}
            </UBadge>
          </div>
          <p v-if="level.health.unbalanced.length > 0" class="mt-1 text-sm text-muted">
            Out of band:
            <span v-for="u in level.health.unbalanced" :key="u.name" class="mr-2">
              {{ u.name }} {{ pct(u.share) }}
            </span>
          </p>
        </li>
      </ul>
    </section>
  </div>
</template>
