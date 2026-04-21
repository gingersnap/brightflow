import { createRouter, createWebHistory } from 'vue-router';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'home',
      component: () => import('./components/layout/WelcomePage.vue'),
    },
    {
      path: '/sources',
      name: 'sources',
      component: () => import('./components/layout/SourceLanding.vue'),
    },
    {
      path: '/sources/new',
      name: 'sources-new',
      component: () => import('./components/sources/NewSourcePage.vue'),
    },
    {
      path: '/connect',
      redirect: { name: 'sources-new' },
    },
    {
      path: '/schedules',
      name: 'schedules',
      component: () => import('./components/schedules/SchedulesView.vue'),
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
