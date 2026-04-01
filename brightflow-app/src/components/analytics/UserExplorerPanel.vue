<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { Search } from 'lucide-vue-next';
import { ref, computed } from 'vue';

import { productAnalyticsApi } from '@/services/api';
import type { UserProfile, UserTimelineEvent } from '@/types';

const props = defineProps<{
  sourceId: string;
}>();

const searchQuery = ref('');
const selectedUserId = ref<string | null>(null);

const { data: users } = useQuery({
  key: () => ['pa-users', props.sourceId, searchQuery.value],
  query: async () => await productAnalyticsApi.searchUsers(props.sourceId, searchQuery.value, 20),
});

const { data: timeline } = useQuery({
  key: () => ['pa-timeline', props.sourceId, selectedUserId.value],
  query: async () => {
    if (!selectedUserId.value) {
      return null;
    }
    return await productAnalyticsApi.userTimeline(props.sourceId, selectedUserId.value);
  },
  enabled: () => Boolean(selectedUserId.value),
});

const { data: profile } = useQuery({
  key: () => ['pa-profile', props.sourceId, selectedUserId.value],
  query: async () => {
    if (!selectedUserId.value) {
      return null;
    }
    return await productAnalyticsApi.userProfile(props.sourceId, selectedUserId.value);
  },
  enabled: () => Boolean(selectedUserId.value),
});

const userList = computed<UserProfile[]>(() => users.value ?? []);
const userTimeline = computed<UserTimelineEvent[]>(() => timeline.value ?? []);

function selectUser(userId: string): void {
  selectedUserId.value = userId;
}

function parseTraits(traitsJson: string): Record<string, unknown> {
  try {
    // oxlint-disable-next-line @typescript-eslint/no-unsafe-return -- JSON parse at boundary
    return JSON.parse(traitsJson);
  } catch {
    return {};
  }
}
</script>

<template>
  <div class="flex gap-6">
    <!-- User list -->
    <div class="w-1/3 min-w-0">
      <div class="relative mb-3">
        <Search class="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted" />
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search users..."
          class="w-full rounded-lg border border-default bg-default py-1.5 pr-3 pl-9 text-sm"
        />
      </div>
      <div class="space-y-1">
        <button
          v-for="user in userList"
          :key="user.userId"
          class="w-full rounded-lg px-3 py-2 text-left text-sm transition-colors"
          :class="
            selectedUserId === user.userId
              ? 'bg-primary-500/10 text-highlighted'
              : 'text-muted hover:bg-elevated'
          "
          @click="selectUser(user.userId)"
        >
          <div class="font-medium text-highlighted">{{ user.userId }}</div>
          <div class="text-xs text-muted">Last seen {{ user.updatedAt.slice(0, 10) }}</div>
        </button>
        <p v-if="userList.length === 0" class="py-4 text-center text-xs text-muted">
          No users found
        </p>
      </div>
    </div>

    <!-- User detail -->
    <div class="min-w-0 flex-1">
      <template v-if="selectedUserId">
        <!-- Traits -->
        <div v-if="profile" class="mb-6 rounded-lg border border-default bg-elevated p-4">
          <h4 class="mb-2 text-sm font-medium text-highlighted">User Traits</h4>
          <div class="space-y-1">
            <div
              v-for="(value, key) in parseTraits(profile.traits)"
              :key="String(key)"
              class="flex justify-between text-sm"
            >
              <span class="text-muted">{{ key }}</span>
              <span class="text-highlighted">{{ value }}</span>
            </div>
            <p
              v-if="Object.keys(parseTraits(profile.traits)).length === 0"
              class="text-xs text-muted"
            >
              No traits set
            </p>
          </div>
        </div>

        <!-- Timeline -->
        <div class="rounded-lg border border-default bg-elevated p-4">
          <h4 class="mb-3 text-sm font-medium text-highlighted">Event Timeline</h4>
          <div v-if="userTimeline.length > 0" class="space-y-2">
            <div
              v-for="(event, i) in userTimeline"
              :key="i"
              class="flex items-start gap-3 border-b border-default pb-2 last:border-0"
            >
              <div
                class="mt-1 h-2 w-2 flex-shrink-0 rounded-full"
                :class="event.eventName === 'pageview' ? 'bg-blue-400' : 'bg-primary-500'"
              />
              <div class="min-w-0 flex-1">
                <div class="flex items-center justify-between">
                  <span class="text-sm font-medium text-highlighted">{{ event.eventName }}</span>
                  <span class="text-xs text-muted">
                    {{ event.timestamp.slice(0, 19).replace('T', ' ') }}
                  </span>
                </div>
                <div v-if="event.pageUrl" class="truncate text-xs text-muted">
                  {{ event.pageUrl }}
                </div>
              </div>
            </div>
          </div>
          <p v-else class="py-4 text-center text-xs text-muted">No events</p>
        </div>
      </template>
      <div v-else class="flex h-full items-center justify-center text-sm text-muted">
        Select a user to view their profile and timeline
      </div>
    </div>
  </div>
</template>
