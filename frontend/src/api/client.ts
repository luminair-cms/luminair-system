import axios, { type InternalAxiosRequestConfig } from 'axios';
import { useAuthStore } from '@/auth/use-auth-store';
import { normalizeAxiosError } from './errors';

// ---------------------------------------------------------------------------
// Primary Export (Configured Axios Client)
// ---------------------------------------------------------------------------

export const apiClient = axios.create({
  baseURL: import.meta.env.VITE_API_BASE_URL || '',
  headers: {
    Accept: 'application/json, application/problem+json',
  },
});

setupInterceptors();

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

function setupInterceptors(): void {
  apiClient.interceptors.request.use((config) => {
    normalizeRequestUrl(config);
    applyAuthHeader(config);
    return config;
  });

  apiClient.interceptors.response.use(
    (response) => response,
    (error) => Promise.reject(normalizeAxiosError(error))
  );
}

function normalizeRequestUrl(config: InternalAxiosRequestConfig): void {
  if (!config.baseURL || !config.url) {
    return;
  }

  const cleanBase = config.baseURL.replace(/\/+$/, '');

  // Root probes (/healthz, /ready) must not inherit an '/api' prefix
  if ((config.url === '/healthz' || config.url === '/ready') && cleanBase.endsWith('/api')) {
    config.baseURL = cleanBase.slice(0, -4);
    return;
  }

  // Prevent double '/api' prefix when baseURL ends with '/api' and url starts with '/api/'
  if (cleanBase.endsWith('/api') && config.url.startsWith('/api/')) {
    config.url = config.url.slice(4);
  }
}

function applyAuthHeader(config: InternalAxiosRequestConfig): void {
  const token = useAuthStore.getState().accessToken;
  if (token && !config.headers.Authorization) {
    config.headers.Authorization = `Bearer ${token}`;
  }
}
