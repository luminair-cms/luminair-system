import { type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from '../client';
import type { ProblemDetails } from '../errors';
import { useSystemConfig, useSystemHealth, useSystemLocales } from '../system.api';
import type { HealthStatusDto, JsonData, LocalesDto, SystemConfigDto } from '../types';

describe('System API Query Hooks', () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
        },
      },
    });
    vi.restoreAllMocks();
  });

  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );

  describe('useSystemConfig', () => {
    it('fetches and unwraps system config data envelope', async () => {
      const mockConfig: SystemConfigDto = {
        id: 'cfg-test-123',
        defaultLocale: 'en',
        availableLocales: ['en', 'uk'],
      };

      vi.spyOn(apiClient, 'get').mockResolvedValue({
        data: { data: mockConfig } as JsonData<SystemConfigDto>,
      });

      const { result } = renderHook(() => useSystemConfig(), { wrapper });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));

      expect(result.current.data).toEqual(mockConfig);
      expect(apiClient.get).toHaveBeenCalledWith('/api/v1/system/config');
    });

    it('propagates ProblemDetails error when request fails', async () => {
      const errorPayload: ProblemDetails = {
        type: 'urn:luminair:error:internal_error',
        title: 'Internal Server Error',
        status: 500,
        code: 'internal_error',
        detail: 'DB failure',
      };

      vi.spyOn(apiClient, 'get').mockRejectedValue(errorPayload);

      const { result } = renderHook(() => useSystemConfig(), { wrapper });

      await waitFor(() => expect(result.current.isError).toBe(true));

      expect(result.current.error).toEqual(errorPayload);
    });
  });

  describe('useSystemLocales', () => {
    it('fetches and unwraps locales data envelope', async () => {
      const mockLocales: LocalesDto = {
        defaultLocale: 'en',
        availableLocales: ['en', 'uk', 'ru'],
      };

      vi.spyOn(apiClient, 'get').mockResolvedValue({
        data: { data: mockLocales } as JsonData<LocalesDto>,
      });

      const { result } = renderHook(() => useSystemLocales(), { wrapper });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));

      expect(result.current.data).toEqual(mockLocales);
      expect(apiClient.get).toHaveBeenCalledWith('/api/v1/system/locales');
    });
  });

  describe('useSystemHealth', () => {
    it('fetches health status probe', async () => {
      const mockHealth: HealthStatusDto = {
        status: 'healthy',
      };

      vi.spyOn(apiClient, 'get').mockResolvedValue({
        data: mockHealth,
      });

      const { result } = renderHook(() => useSystemHealth(), { wrapper });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));

      expect(result.current.data).toEqual(mockHealth);
      expect(apiClient.get).toHaveBeenCalledWith('/healthz');
    });
  });
});
