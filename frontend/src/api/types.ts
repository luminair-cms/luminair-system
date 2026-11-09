/**
 * Standard API response envelopes and core system DTOs.
 *
 * Conforms to infrastructure/src/api/envelope.rs and response codecs.
 */

// ---------------------------------------------------------------------------
// Envelope Types
// ---------------------------------------------------------------------------

/**
 * Standard success envelope wrapping payload data.
 */
export interface JsonData<T> {
  data: T;
}

/**
 * Resource creation envelope returning created payload data.
 */
export interface CreatedJson<T> {
  data: T;
}

/**
 * Pagination metadata accompanying paged collections.
 */
export interface PaginationMeta {
  page: number;
  pageSize: number;
  total: number;
  totalPages: number;
}

/**
 * Paginated collection envelope returning items and pagination metadata.
 */
export interface PagedJson<T> {
  data: T[];
  meta: PaginationMeta;
}

// ---------------------------------------------------------------------------
// System DTO Types
// ---------------------------------------------------------------------------

/**
 * Active system configuration including locales and identifier.
 */
export interface SystemConfigDto {
  id: string;
  defaultLocale: string;
  availableLocales: string[];
}

/**
 * Default and available system locales.
 */
export interface LocalesDto {
  defaultLocale: string;
  availableLocales: string[];
}

/**
 * Health probe status reported by liveness and readiness checks.
 */
export interface HealthStatusDto {
  status: 'healthy' | 'unhealthy' | string;
  reason?: string;
}
