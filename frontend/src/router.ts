import { createRouter } from '@tanstack/react-router';
import { Route as rootRoute } from './routes/__root';
import { Route as indexRoute } from './routes/index';

// ---------------------------------------------------------------------------
// Route Tree & Router Configuration
// ---------------------------------------------------------------------------

const routeTree = rootRoute.addChildren([indexRoute]);

export const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
});

// ---------------------------------------------------------------------------
// Type Registration
// ---------------------------------------------------------------------------

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
