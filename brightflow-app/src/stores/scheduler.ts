import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { SchedulerJob, SyncRun } from '@/services/schedulerApi';

export const useSchedulerStore = defineStore('scheduler', () => {
  const jobs = ref<SchedulerJob[]>([]);
  const runs = ref<SyncRun[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  let pollInterval: ReturnType<typeof setInterval> | null = null;

  async function fetchJobs(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const { schedulerJobApi } = await import('@/services/schedulerApi');
      const result = await schedulerJobApi.list();
      jobs.value = result ?? [];
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load jobs';
    } finally {
      loading.value = false;
    }
  }

  async function fetchRuns(): Promise<void> {
    try {
      const { syncApi } = await import('@/services/schedulerApi');
      const result = await syncApi.listRuns();
      runs.value = result ?? [];
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load runs';
    }
  }

  async function createJob(name: string, connectorId: string, intervalSecs: number): Promise<void> {
    error.value = null;
    try {
      const { schedulerJobApi } = await import('@/services/schedulerApi');
      await schedulerJobApi.create({ name, connectorId, intervalSecs });
      await fetchJobs();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to create job';
    }
  }

  async function updateJob(
    id: string,
    data: { intervalSecs?: number; enabled?: boolean },
  ): Promise<void> {
    error.value = null;
    try {
      const { schedulerJobApi } = await import('@/services/schedulerApi');
      await schedulerJobApi.update(id, data);
      await fetchJobs();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to update job';
    }
  }

  async function deleteJob(id: string): Promise<void> {
    error.value = null;
    try {
      const { schedulerJobApi } = await import('@/services/schedulerApi');
      await schedulerJobApi.delete(id);
      await fetchJobs();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to delete job';
    }
  }

  async function triggerRun(jobId: string): Promise<void> {
    error.value = null;
    try {
      const { schedulerJobApi } = await import('@/services/schedulerApi');
      await schedulerJobApi.triggerRun(jobId);
      startPolling();
      await fetchRuns();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to trigger run';
    }
  }

  function startPolling(): void {
    stopPolling();
    pollInterval = setInterval(async () => {
      await fetchRuns();

      // Stop polling once no runs are pending/running
      const hasActive = runs.value.some((r) => r.status === 'pending' || r.status === 'running');
      if (!hasActive) {
        stopPolling();
      }
    }, 3000);
  }

  function stopPolling(): void {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function latestRunForJob(jobId: string): SyncRun | undefined {
    return runs.value.find((r) => r.jobId === jobId);
  }

  function latestRunForConnector(connectorId: string): SyncRun | undefined {
    return runs.value.find((r) => r.connectorId === connectorId);
  }

  return {
    jobs,
    runs,
    loading,
    error,
    fetchJobs,
    fetchRuns,
    createJob,
    updateJob,
    deleteJob,
    triggerRun,
    startPolling,
    stopPolling,
    latestRunForJob,
    latestRunForConnector,
  };
});
