import axios, { type AxiosError } from 'axios';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

/**
 * Field-level validation failure descriptor (RFC 9457 extension).
 */
export interface InvalidParam {
  name: string;
  reason: string;
  code: string;
}

/**
 * Canonical RFC 9457 Problem Details error payload.
 */
export interface ProblemDetails {
  type: string;
  title: string;
  status: number;
  code: string;
  detail: string;
  instance?: string;
  details?: Record<string, unknown>;
  invalid_params?: InvalidParam[];
}

// ---------------------------------------------------------------------------
// Primary Exports (Type Guards & Normalizers)
// ---------------------------------------------------------------------------

/**
 * Type guard verifying whether an unknown value conforms to RFC 9457 `ProblemDetails`.
 */
export function isProblemDetails(error: unknown): error is ProblemDetails {
  if (typeof error !== 'object' || error === null) {
    return false;
  }

  const candidate = error as Record<string, unknown>;
  return (
    typeof candidate.type === 'string' &&
    typeof candidate.title === 'string' &&
    typeof candidate.status === 'number' &&
    typeof candidate.code === 'string' &&
    typeof candidate.detail === 'string'
  );
}

/**
 * Normalizes any error (Axios error, standard Error, or unknown) into a canonical `ProblemDetails`.
 */
export function normalizeAxiosError(error: unknown): ProblemDetails {
  if (isProblemDetails(error)) {
    return error;
  }

  if (axios.isAxiosError(error)) {
    return normalizeFromAxios(error);
  }

  return normalizeFromUnknown(error);
}

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

function normalizeFromAxios(error: AxiosError): ProblemDetails {
  if (error.response) {
    return normalizeResponseError(error.response.status, error.response.data, error.config?.url);
  }

  if (error.request) {
    return createProblemDetails({
      code: 'network_error',
      title: 'Network Error',
      status: 0,
      detail: error.message || 'Unable to connect to the server. Please check your network connection.',
      instance: error.config?.url,
    });
  }

  return createProblemDetails({
    code: 'unknown_error',
    title: 'Client Request Error',
    status: 0,
    detail: error.message || 'Failed to issue network request.',
    instance: error.config?.url,
  });
}

function normalizeResponseError(status: number, data: unknown, instance?: string): ProblemDetails {
  if (isProblemDetails(data)) {
    return {
      ...data,
      instance: data.instance ?? instance,
    };
  }

  const dataObj = typeof data === 'object' && data !== null ? (data as Record<string, unknown>) : null;
  const code = typeof dataObj?.code === 'string' ? dataObj.code : `http_${status}`;
  const title = typeof dataObj?.title === 'string' ? dataObj.title : `HTTP ${status}`;
  const detail = typeof dataObj?.detail === 'string'
    ? dataObj.detail
    : typeof dataObj?.message === 'string'
      ? dataObj.message
      : `HTTP request failed with status code ${status}`;

  return createProblemDetails({
    code,
    title,
    status,
    detail,
    instance,
    details: dataObj?.details as Record<string, unknown> | undefined,
  });
}

function normalizeFromUnknown(error: unknown): ProblemDetails {
  if (error instanceof Error) {
    return createProblemDetails({
      code: 'unknown_error',
      title: 'Unexpected Error',
      status: 0,
      detail: error.message,
    });
  }

  return createProblemDetails({
    code: 'unknown_error',
    title: 'Unexpected Error',
    status: 0,
    detail: String(error ?? 'An unexpected error occurred.'),
  });
}

function createProblemDetails(params: {
  code: string;
  title: string;
  status: number;
  detail: string;
  instance?: string;
  details?: Record<string, unknown>;
  invalid_params?: InvalidParam[];
}): ProblemDetails {
  return {
    type: `urn:luminair:error:${params.code}`,
    title: params.title,
    status: params.status,
    code: params.code,
    detail: params.detail,
    ...(params.instance ? { instance: params.instance } : {}),
    ...(params.details ? { details: params.details } : {}),
    ...(params.invalid_params ? { invalid_params: params.invalid_params } : {}),
  };
}
