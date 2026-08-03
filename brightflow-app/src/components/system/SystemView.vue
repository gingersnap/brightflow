<script setup lang="ts">
/**
 * Live server health view: metric cards (memory, CPU, uptime) and a
 * tailing log feed, fed by the system store's connection which this view
 * opens on mount and closes on unmount. Auto-scroll follows new log
 * entries but yields as soon as the user scrolls up, with a
 * "scroll to bottom" button to re-pin.
 */

import { Trash2 } from '@lucide/vue';
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';

import { useSystemStore } from '@/stores/system';

const systemStore = useSystemStore();
const logContainer = ref<HTMLElement | null>(null);
const autoScroll = ref(true);

onMounted(() => {
  systemStore.connect();
});

onUnmounted(() => {
  systemStore.disconnect();
});

function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const val = bytes / 1024 ** i;
  return `${val.toFixed(val >= 100 ? 0 : 1)} ${units[i]}`;
}

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) {
    return `${h}h ${m}m`;
  }
  if (m > 0) {
    return `${m}m ${s}s`;
  }
  return `${s}s`;
}

function formatCpu(pct: number): string {
  return `${pct.toFixed(1)}%`;
}

const memoryPercent = computed(() => {
  const m = systemStore.metrics;
  if (!m || m.systemTotalBytes === 0) {
    return 0;
  }
  return Math.round((m.systemUsedBytes / m.systemTotalBytes) * 100);
});

function levelColor(level: string): string {
  switch (level) {
    case 'ERROR': {
      return 'bg-red-500/15 text-red-500';
    }
    case 'WARN': {
      return 'bg-yellow-500/15 text-yellow-600 dark:text-yellow-400';
    }
    case 'INFO': {
      return 'bg-blue-500/15 text-blue-500';
    }
    default: {
      return 'bg-neutral-500/15 text-neutral-500';
    }
  }
}

function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return ts;
  }
}

// Auto-scroll to bottom when new logs arrive
watch(
  () => systemStore.logs.length,
  async () => {
    if (autoScroll.value && logContainer.value) {
      await nextTick();
      logContainer.value.scrollTop = logContainer.value.scrollHeight;
    }
  },
);

// Detect manual scroll to disable auto-scroll
function handleScroll(): void {
  if (!logContainer.value) {
    return;
  }
  const el = logContainer.value;
  const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  autoScroll.value = atBottom;
}
</script>

<template>
  <UDashboardPanel id="system">
    <template #header>
      <UDashboardNavbar title="System">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
        <template #trailing>
          <span
            class="h-2 w-2 rounded-full"
            :class="{
              'bg-green-500': systemStore.isConnected,
              'animate-pulse bg-yellow-500': systemStore.status === 'connecting',
              'bg-neutral-400': !systemStore.isConnected && systemStore.status !== 'connecting',
            }"
          />
          <span class="text-sm text-muted">{{ systemStore.status }}</span>

          <UButton variant="ghost" size="md" @click="systemStore.clearLogs()">
            <Trash2 class="h-3.5 w-3.5" />
            Clear
          </UButton>
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <!-- Metrics cards -->
      <div
        v-if="systemStore.metrics"
        class="grid grid-cols-4 gap-3 border-b border-default bg-elevated/50 px-4 py-3"
      >
        <!-- Process Memory -->
        <div class="rounded-lg border border-default bg-default p-3">
          <div class="mb-1 text-sm text-muted">Process Memory</div>
          <div class="text-lg font-semibold text-highlighted">
            {{ formatBytes(systemStore.metrics.processRssBytes) }}
            <span class="text-xs font-normal text-muted"
              >({{ formatBytes(systemStore.metrics.processAnonBytes) }} private)</span
            >
          </div>
        </div>

        <!-- System Memory -->
        <div class="rounded-lg border border-default bg-default p-3">
          <div class="mb-1 text-sm text-muted">System Memory</div>
          <div class="mb-1.5 text-sm font-semibold text-highlighted">
            {{ formatBytes(systemStore.metrics.systemUsedBytes) }}
            <span class="text-xs font-normal text-muted"
              >/ {{ formatBytes(systemStore.metrics.systemTotalBytes) }}</span
            >
          </div>
          <div class="h-1.5 w-full overflow-hidden rounded-full bg-elevated">
            <div
              class="h-full rounded-full transition-all duration-500"
              :class="
                memoryPercent > 80
                  ? 'bg-red-500'
                  : memoryPercent > 60
                    ? 'bg-yellow-500'
                    : 'bg-primary-500'
              "
              :style="{ width: `${memoryPercent}%` }"
            />
          </div>
        </div>

        <!-- Process CPU -->
        <div class="rounded-lg border border-default bg-default p-3">
          <div class="mb-1 text-sm text-muted">Process CPU</div>
          <div class="text-lg font-semibold text-highlighted">
            {{ formatCpu(systemStore.metrics.cpuPercent) }}
          </div>
        </div>

        <!-- Uptime -->
        <div class="rounded-lg border border-default bg-default p-3">
          <div class="mb-1 text-sm text-muted">Uptime</div>
          <div class="text-lg font-semibold text-highlighted">
            {{ formatUptime(systemStore.metrics.uptimeSecs) }}
          </div>
        </div>
      </div>

      <!-- Log feed -->
      <div
        ref="logContainer"
        class="min-h-0 flex-1 overflow-y-auto font-mono text-xs"
        @scroll="handleScroll"
      >
        <div
          v-if="systemStore.logs.length === 0"
          class="flex h-full items-center justify-center text-muted"
        >
          Waiting for log entries...
        </div>

        <div
          v-for="(entry, i) in systemStore.logs"
          :key="i"
          class="flex items-start gap-2 border-b border-default/50 px-4 py-1 hover:bg-elevated/50"
        >
          <span class="w-18 shrink-0 text-muted">{{ formatTimestamp(entry.timestamp) }}</span>
          <span
            class="shrink-0 rounded px-1.5 py-0.5 text-[10px] leading-none font-semibold"
            :class="levelColor(entry.level)"
          >
            {{ entry.level }}
          </span>
          <span class="max-w-48 shrink-0 truncate text-muted">{{ entry.target }}</span>
          <span class="break-all text-highlighted">{{ entry.message }}</span>
        </div>
      </div>

      <!-- Auto-scroll indicator -->
      <div v-if="!autoScroll && systemStore.logs.length > 0" class="absolute right-4 bottom-4">
        <UButton size="md" variant="solid" @click="autoScroll = true"> Scroll to bottom </UButton>
      </div>
    </template>
  </UDashboardPanel>
</template>
