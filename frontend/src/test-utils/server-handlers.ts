import { http, HttpResponse } from 'msw';
import type { HealthStatusDto, JsonData, LocalesDto, SystemConfigDto } from '@/api/types';

// ---------------------------------------------------------------------------
// Constants & Mock Fixtures
// ---------------------------------------------------------------------------

export const mockSystemConfig: SystemConfigDto = {
  id: 'cfg-default-001',
  defaultLocale: 'en',
  availableLocales: ['en', 'uk'],
};

export const mockSystemLocales: LocalesDto = {
  defaultLocale: 'en',
  availableLocales: ['en', 'uk'],
};

export const mockHealthStatus: HealthStatusDto = {
  status: 'healthy',
};

// ---------------------------------------------------------------------------
// Primary Export (Default MSW Request Handlers)
// ---------------------------------------------------------------------------

export const handlers = [
  http.get('/api/v1/system/config', () => {
    const payload: JsonData<SystemConfigDto> = { data: mockSystemConfig };
    return HttpResponse.json(payload);
  }),

  http.get('/api/v1/system/locales', () => {
    const payload: JsonData<LocalesDto> = { data: mockSystemLocales };
    return HttpResponse.json(payload);
  }),

  http.get('/healthz', () => {
    return HttpResponse.json(mockHealthStatus);
  }),
];
