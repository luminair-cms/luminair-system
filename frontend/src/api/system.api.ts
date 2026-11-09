import { useQuery, type UseQueryResult } from '@tanstack/react-query';
import { apiClient } from './client';
import type { ProblemDetails } from './errors';
import { queryKeys } from './query-keys';
import type { HealthStatusDto, JsonData, LocalesDto, SystemConfigDto } from './types';

// ---------------------------------------------------------------------------
// Primary Exports (Query Hooks)
// ---------------------------------------------------------------------------

/**
 * Fetches active system configuration (default locale, available locales, ID).
 */
export function useSystemConfig(): UseQueryResult<SystemConfigDto, ProblemDetails> {
  return useQuery<SystemConfigDto, ProblemDetails>({
    queryKey: queryKeys.system.config(),
    queryFn: fetchSystemConfig,
    staleTime: 5 * 60 * 1000,
  });
}

/**
 * Fetches system locale configuration.
 */
export function useSystemLocales(): UseQueryResult<LocalesDto, ProblemDetails> {
  return useQuery<LocalesDto, ProblemDetails>({
    queryKey: queryKeys.system.locales(),
    queryFn: fetchSystemLocales,
    staleTime: 5 * 60 * 1000,
  });
}

/**
 * Fetches server liveness status probe.
 */
export function useSystemHealth(): UseQueryResult<HealthStatusDto, ProblemDetails> {
  return useQuery<HealthStatusDto, ProblemDetails>({
    queryKey: queryKeys.system.health(),
    queryFn: fetchSystemHealth,
    staleTime: 10 * 1000,
  });
}

// ---------------------------------------------------------------------------
// Internal Helpers (Fetchers)
// ---------------------------------------------------------------------------

async function fetchSystemConfig(): Promise<SystemConfigDto> {
  const response = await apiClient.get<JsonData<SystemConfigDto>>('/api/v1/system/config');
  return response.data.data;
}

async function fetchSystemLocales(): Promise<LocalesDto> {
  const response = await apiClient.get<JsonData<LocalesDto>>('/api/v1/system/locales');
  return response.data.data;
}

async function fetchSystemHealth(): Promise<HealthStatusDto> {
  const response = await apiClient.get<HealthStatusDto>('/healthz');
  return response.data;
}
