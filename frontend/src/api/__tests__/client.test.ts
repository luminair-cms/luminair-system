import { AxiosError, type AxiosResponse, type InternalAxiosRequestConfig } from 'axios';
import { beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '@/auth/use-auth-store';
import { apiClient } from '../client';
import type { ProblemDetails } from '../errors';

describe('apiClient', () => {
  beforeEach(() => {
    useAuthStore.getState().clearSession();
  });

  describe('Authorization header injection', () => {
    it('injects Bearer token when user is authenticated in useAuthStore', async () => {
      useAuthStore.getState().setSession(
        { access_token: 'jwt-access-token-123' },
        { sub: 'usr-1', email: 'editor@luminair.io' }
      );

      let capturedConfig: InternalAxiosRequestConfig | undefined;
      apiClient.defaults.adapter = async (config) => {
        capturedConfig = config;
        return {
          data: { status: 'ok' },
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
        } as AxiosResponse;
      };

      await apiClient.get('/api/v1/system/config');

      expect(capturedConfig?.headers.Authorization).toBe('Bearer jwt-access-token-123');
    });

    it('does not set Authorization header when no access token exists', async () => {
      let capturedConfig: InternalAxiosRequestConfig | undefined;
      apiClient.defaults.adapter = async (config) => {
        capturedConfig = config;
        return {
          data: { status: 'ok' },
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
        } as AxiosResponse;
      };

      await apiClient.get('/api/v1/system/config');

      expect(capturedConfig?.headers.Authorization).toBeUndefined();
    });

    it('preserves existing custom Authorization header', async () => {
      useAuthStore.getState().setSession(
        { access_token: 'store-token' },
        { sub: 'usr-1' }
      );

      let capturedConfig: InternalAxiosRequestConfig | undefined;
      apiClient.defaults.adapter = async (config) => {
        capturedConfig = config;
        return {
          data: { status: 'ok' },
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
        } as AxiosResponse;
      };

      await apiClient.get('/api/v1/system/config', {
        headers: { Authorization: 'Bearer custom-token' },
      });

      expect(capturedConfig?.headers.Authorization).toBe('Bearer custom-token');
    });
  });

  describe('URL normalization and baseURL deduplication', () => {
    it('strips redundant /api prefix from request URL when baseURL ends with /api', async () => {
      let capturedConfig: InternalAxiosRequestConfig | undefined;
      apiClient.defaults.adapter = async (config) => {
        capturedConfig = config;
        return {
          data: { status: 'ok' },
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
        } as AxiosResponse;
      };

      const prevBase = apiClient.defaults.baseURL;
      apiClient.defaults.baseURL = '/api';

      try {
        await apiClient.get('/api/v1/system/config');
        expect(capturedConfig?.url).toBe('/v1/system/config');
      } finally {
        apiClient.defaults.baseURL = prevBase;
      }
    });

    it('adjusts baseURL for root health probe endpoints (/healthz, /ready)', async () => {
      let capturedConfig: InternalAxiosRequestConfig | undefined;
      apiClient.defaults.adapter = async (config) => {
        capturedConfig = config;
        return {
          data: { status: 'healthy' },
          status: 200,
          statusText: 'OK',
          headers: {},
          config,
        } as AxiosResponse;
      };

      const prevBase = apiClient.defaults.baseURL;
      apiClient.defaults.baseURL = '/api';

      try {
        await apiClient.get('/healthz');
        expect(capturedConfig?.baseURL).toBe('');
        expect(capturedConfig?.url).toBe('/healthz');
      } finally {
        apiClient.defaults.baseURL = prevBase;
      }
    });
  });

  describe('RFC 9457 response error normalization', () => {
    it('intercepts HTTP error and rejects with normalized ProblemDetails', async () => {
      const problemPayload: ProblemDetails = {
        type: 'urn:luminair:error:version_mismatch',
        title: 'Conflict',
        status: 409,
        code: 'version_mismatch',
        detail: 'Document version modified by another user.',
        details: { expected_version: 2, found_version: 3 },
      };

      apiClient.defaults.adapter = async (config) => {
        const response: AxiosResponse = {
          data: problemPayload,
          status: 409,
          statusText: 'Conflict',
          headers: {},
          config,
        };
        throw new AxiosError('Request failed with status code 409', 'ERR_BAD_REQUEST', config, undefined, response);
      };

      await expect(apiClient.get('/api/v1/content/articles/123')).rejects.toMatchObject({
        type: 'urn:luminair:error:version_mismatch',
        code: 'version_mismatch',
        status: 409,
        title: 'Conflict',
        detail: 'Document version modified by another user.',
        details: { expected_version: 2, found_version: 3 },
      });
    });
  });
});
