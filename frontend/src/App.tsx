import type { ReactElement } from 'react';
import { MantineProvider } from '@mantine/core';
import { ModalsProvider } from '@mantine/modals';
import { Notifications } from '@mantine/notifications';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { router as appRouter } from './router';
import { theme } from './theme';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface AppProps {
  router?: typeof appRouter;
  queryClient?: QueryClient;
}

// ---------------------------------------------------------------------------
// Primary Export (Application Root with Providers)
// ---------------------------------------------------------------------------

export function App({
  router = appRouter,
  queryClient = defaultQueryClient,
}: AppProps): ReactElement {
  return (
    <QueryClientProvider client={queryClient}>
      <MantineProvider theme={theme}>
        <ModalsProvider>
          <Notifications />
          <RouterProvider router={router} />
        </ModalsProvider>
      </MantineProvider>
    </QueryClientProvider>
  );
}

// ---------------------------------------------------------------------------
// Constants & Static Metadata
// ---------------------------------------------------------------------------

export const defaultQueryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60 * 1000,
      retry: 1,
    },
  },
});
