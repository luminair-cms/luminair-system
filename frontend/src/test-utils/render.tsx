import { type ReactElement, type ReactNode } from 'react';
import { MantineProvider } from '@mantine/core';
import { ModalsProvider } from '@mantine/modals';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render as rtlRender, type RenderOptions, type RenderResult } from '@testing-library/react';
import { theme } from '@/theme';
import '@/i18n/config';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface ProviderWrapperProps {
  children: ReactNode;
}

export interface RenderWithProvidersOptions extends Omit<RenderOptions, 'wrapper'> {
  queryClient?: QueryClient;
}

// ---------------------------------------------------------------------------
// Primary Exports
// ---------------------------------------------------------------------------

/**
 * Creates a QueryClient wrapper configured with testing defaults (no retries, gcTime 0).
 */
export function createQueryWrapper(
  client?: QueryClient
): ({ children }: ProviderWrapperProps) => ReactElement {
  const queryClient = client ?? createTestQueryClient();
  return function QueryWrapper({ children }: ProviderWrapperProps): ReactElement {
    return (
      <QueryClientProvider client={queryClient}>
        <MantineProvider theme={theme}>
          <ModalsProvider>{children}</ModalsProvider>
        </MantineProvider>
      </QueryClientProvider>
    );
  };
}

/**
 * Renders a React component wrapped with all required application providers:
 * QueryClientProvider, MantineProvider, and ModalsProvider.
 */
export function renderWithProviders(
  ui: ReactElement,
  options?: RenderWithProvidersOptions
): RenderResult {
  const client = options?.queryClient ?? createTestQueryClient();
  const { queryClient: _ignored, ...rtlOptions } = options ?? {};

  return rtlRender(ui, {
    wrapper: createQueryWrapper(client),
    ...rtlOptions,
  });
}

/**
 * Creates a test QueryClient instance isolated for individual test runs.
 */
export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
      },
      mutations: {
        retry: false,
      },
    },
  });
}
