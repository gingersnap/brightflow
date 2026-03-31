<script setup lang="ts">
import { Trash2 } from 'lucide-vue-next';
import { computed, nextTick, ref, watch } from 'vue';

import { useSystemStore } from '@/stores/system';

const systemStore = useSystemStore();
const logContainer = ref<HTMLElement | null>(null);
const autoScroll = ref(true);

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
  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="flex items-center gap-3 px-4 py-2.5 border-b border-default bg-default">
      <h2 class="text-sm font-semibold text-highlighted">System</h2>

      <div class="flex-1" />

      <span
        class="h-2 w-2 rounded-full"
        :class="{
          'bg-green-500': systemStore.isConnected,
          'bg-yellow-500 animate-pulse': systemStore.status === 'connecting',
          'bg-neutral-400': !systemStore.isConnected && systemStore.status !== 'connecting',
        }"
      />
      <span class="text-xs text-muted">{{ systemStore.status }}</span>

      <UButton variant="ghost" size="xs" @click="systemStore.clearLogs()">
        <Trash2 class="w-3.5 h-3.5" />
        Clear
      </UButton>
    </div>

    <!-- Metrics cards -->
    <div
      v-if="systemStore.metrics"
      class="grid grid-cols-4 gap-3 px-4 py-3 border-b border-default bg-elevated/50"
    >
      <!-- Process Memory -->
      <div class="rounded-lg border border-default bg-default p-3">
        <div class="text-xs text-muted mb-1">Process Memory</div>
        <div class="text-lg font-semibold text-highlighted">
          {{ formatBytes(systemStore.metrics.processRssBytes) }}
          <span class="text-xs text-muted font-normal"
            >({{ formatBytes(systemStore.metrics.processAnonBytes) }} private)</span
          >
        </div>
      </div>

      <!-- System Memory -->
      <div class="rounded-lg border border-default bg-default p-3">
        <div class="text-xs text-muted mb-1">System Memory</div>
        <div class="text-sm font-semibold text-highlighted mb-1.5">
          {{ formatBytes(systemStore.metrics.systemUsedBytes) }}
          <span class="text-xs text-muted font-normal"
            >/ {{ formatBytes(systemStore.metrics.systemTotalBytes) }}</span
          >
        </div>
        <div class="w-full h-1.5 bg-elevated rounded-full overflow-hidden">
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
        <div class="text-xs text-muted mb-1">Process CPU</div>
        <div class="text-lg font-semibold text-highlighted">
          {{ formatCpu(systemStore.metrics.cpuPercent) }}
        </div>
      </div>

      <!-- Uptime -->
      <div class="rounded-lg border border-default bg-default p-3">
        <div class="text-xs text-muted mb-1">Uptime</div>
        <div class="text-lg font-semibold text-highlighted">
          {{ formatUptime(systemStore.metrics.uptimeSecs) }}
        </div>
      </div>
    </div>

    <!-- Log feed -->
    <div
      ref="logContainer"
      class="flex-1 min-h-0 overflow-y-auto font-mono text-xs"
      @scroll="handleScroll"
    >
      <div
        v-if="systemStore.logs.length === 0"
        class="flex items-center justify-center h-full text-muted"
      >
        Waiting for log entries...
      </div>

      <div
        v-for="(entry, i) in systemStore.logs"
        :key="i"
        class="flex items-start gap-2 px-4 py-1 border-b border-default/50 hover:bg-elevated/50"
      >
        <span class="text-muted shrink-0 w-18">{{ formatTimestamp(entry.timestamp) }}</span>
        <span
          class="shrink-0 px-1.5 py-0.5 rounded text-[10px] font-semibold leading-none"
          :class="levelColor(entry.level)"
        >
          {{ entry.level }}
        </span>
        <span class="text-muted shrink-0 max-w-48 truncate">{{ entry.target }}</span>
        <span class="text-highlighted break-all">{{ entry.message }}</span>
      </div>
    </div>

    <!-- Auto-scroll indicator -->
    <div v-if="!autoScroll && systemStore.logs.length > 0" class="absolute bottom-4 right-4">
      <UButton size="xs" variant="solid" @click="autoScroll = true"> Scroll to bottom </UButton>
    </div>
  </div>
</template>
