import { createRouter, createWebHistory } from 'vue-router';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'sources',
      component: () => import('./components/layout/SourceLanding.vue'),
    },
    {
      path: '/connect',
      name: 'connect',
      component: () => import('./components/connect/ConnectView.vue'),
    },
    {
      path: '/system',
      name: 'system',
      component: () => import('./components/system/SystemView.vue'),
    },
    {
      path: '/:sourceId/insights/:table',
      name: 'insights-table',
      component: () => import('./components/layout/SourceLayout.vue'),
      props: (route) => ({
        sourceId: String(route.params['sourceId'] ?? ''),
        tool: 'insights' as const,
        table: String(route.params['table'] ?? ''),
      }),
    },
    {
      path: '/:sourceId/explore/:table',
      name: 'explore-table',
      component: () => import('./components/layout/SourceLayout.vue'),
      props: (route) => ({
        sourceId: String(route.params['sourceId'] ?? ''),
        tool: 'explore' as const,
        table: String(route.params['table'] ?? ''),
      }),
    },
    {
      path: '/:sourceId/:tool',
      name: 'source-tool',
      component: () => import('./components/layout/SourceLayout.vue'),
      props: (route) => ({
        sourceId: String(route.params['sourceId'] ?? ''),
        tool: String(route.params['tool'] ?? '') || 'dashboard',
      }),
    },
    {
      // Redirect bare sourceId to dashboard
      path: '/:sourceId',
      redirect: (to) => ({
        name: 'source-tool',
        params: { sourceId: to.params['sourceId'], tool: 'dashboard' },
      }),
    },
  ],
});

export default router;
