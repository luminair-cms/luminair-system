import { AxiosError, type AxiosResponse, type InternalAxiosRequestConfig } from 'axios';
import { describe, expect, it } from 'vitest';
import { isProblemDetails, normalizeAxiosError, type ProblemDetails } from '../errors';

describe('isProblemDetails', () => {
  it('returns true for a compliant RFC 9457 ProblemDetails object', () => {
    const valid: ProblemDetails = {
      type: 'urn:luminair:error:draft_has_pending_edits',
      title: 'Conflict',
      status: 409,
      code: 'draft_has_pending_edits',
      detail: 'Restoring snapshot would overwrite pending working draft edits',
      instance: '/api/v1/content/articles/123/restore',
      details: { revision: 1 },
    };

    expect(isProblemDetails(valid)).toBe(true);
  });

  it('returns false for null or primitives', () => {
    expect(isProblemDetails(null)).toBe(false);
    expect(isProblemDetails(undefined)).toBe(false);
    expect(isProblemDetails('error string')).toBe(false);
    expect(isProblemDetails(404)).toBe(false);
  });

  it('returns false for an object missing required fields', () => {
    expect(isProblemDetails({ code: 'some_error', message: 'failed' })).toBe(false);
    expect(isProblemDetails({ type: 'urn:test', title: 'Error', status: 500, detail: 'Failed' })).toBe(false);
    expect(isProblemDetails({ title: 'Error', status: 500, code: 'err', detail: 'Failed' })).toBe(false);
  });
});

describe('normalizeAxiosError', () => {
  it('returns an existing ProblemDetails instance directly', () => {
    const original: ProblemDetails = {
      type: 'urn:luminair:error:already_published',
      title: 'Conflict',
      status: 409,
      code: 'already_published',
      detail: 'This document is already published.',
    };

    const result = normalizeAxiosError(original);
    expect(result).toEqual(original);
  });

  it('normalizes an Axios error with RFC 9457 response payload', () => {
    const payload: ProblemDetails = {
      type: 'urn:luminair:error:validation_failed',
      title: 'Unprocessable Entity',
      status: 422,
      code: 'validation_failed',
      detail: 'Validation failed',
      invalid_params: [{ name: 'title', reason: 'is required', code: 'required_field_missing' }],
    };

    const axiosError = createAxiosErrorWithResponse(422, payload, '/api/v1/content/articles');
    const result = normalizeAxiosError(axiosError);

    expect(result.status).toBe(422);
    expect(result.code).toBe('validation_failed');
    expect(result.title).toBe('Unprocessable Entity');
    expect(result.invalid_params).toHaveLength(1);
    expect(result.instance).toBe('/api/v1/content/articles');
  });

  it('normalizes an Axios error with generic JSON response', () => {
    const axiosError = createAxiosErrorWithResponse(500, { message: 'Database crash' });
    const result = normalizeAxiosError(axiosError);

    expect(result.status).toBe(500);
    expect(result.code).toBe('http_500');
    expect(result.detail).toBe('Database crash');
    expect(result.type).toBe('urn:luminair:error:http_500');
  });

  it('normalizes an Axios error with plain text/HTML response', () => {
    const axiosError = createAxiosErrorWithResponse(502, 'Bad Gateway');
    const result = normalizeAxiosError(axiosError);

    expect(result.status).toBe(502);
    expect(result.code).toBe('http_502');
    expect(result.detail).toBe('HTTP request failed with status code 502');
  });

  it('normalizes a network failure without response', () => {
    const axiosError = new AxiosError(
      'Network Error: Connection refused',
      'ERR_NETWORK',
      { url: '/api/v1/system/config' } as InternalAxiosRequestConfig,
      {} // request present
    );

    const result = normalizeAxiosError(axiosError);
    expect(result.status).toBe(0);
    expect(result.code).toBe('network_error');
    expect(result.title).toBe('Network Error');
    expect(result.instance).toBe('/api/v1/system/config');
  });

  it('normalizes an unknown client setup error', () => {
    const axiosError = new AxiosError('Invalid configuration');
    const result = normalizeAxiosError(axiosError);

    expect(result.status).toBe(0);
    expect(result.code).toBe('unknown_error');
  });

  it('normalizes standard Error objects and unexpected primitives', () => {
    const errorResult = normalizeAxiosError(new Error('Manual boom'));
    expect(errorResult.code).toBe('unknown_error');
    expect(errorResult.detail).toBe('Manual boom');

    const stringResult = normalizeAxiosError('Unexpected string error');
    expect(stringResult.code).toBe('unknown_error');
    expect(stringResult.detail).toBe('Unexpected string error');
  });
});

// ---------------------------------------------------------------------------
// Test Helpers
// ---------------------------------------------------------------------------

function createAxiosErrorWithResponse(status: number, data: unknown, url = '/test-url'): AxiosError {
  const config = { url } as InternalAxiosRequestConfig;
  const response: AxiosResponse = {
    data,
    status,
    statusText: status === 422 ? 'Unprocessable Entity' : 'Internal Server Error',
    headers: {},
    config,
  };

  return new AxiosError('Request failed with status code ' + status, undefined, config, undefined, response);
}
