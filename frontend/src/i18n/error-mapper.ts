import { notifications } from '@mantine/notifications';
import type { ProblemDetails } from '@/api/errors';
import { i18n } from './config';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export type TranslationFn = (key: string, options?: Record<string, unknown>) => string;

// ---------------------------------------------------------------------------
// Primary Exports
// ---------------------------------------------------------------------------

/**
 * Resolves a localized error message from RFC 9457 `ProblemDetails`.
 *
 * Conforms to .ai/rules/frontend-react.md §5:
 * The UI client owns end-user localization. Always map error.code to the translation
 * catalog with error.details interpolation. Never display raw backend detail to users.
 */
export function getLocalizedErrorMessage(error: ProblemDetails, t?: TranslationFn): string {
  const translate = t ?? createDefaultTranslator();
  const i18nKey = `errors.${error.code}`;

  if (i18n.exists(i18nKey)) {
    return translate(i18nKey, error.details);
  }

  if (i18n.exists('errors.unknown_error')) {
    return translate('errors.unknown_error');
  }

  return 'An unexpected error occurred.';
}

/**
 * Maps RFC 9457 validation errors (422 invalid_params) to a key-value record
 * formatted for Mantine form error binding: `form.setErrors(...)`.
 */
export function mapValidationErrorsToForm(error: ProblemDetails): Record<string, string> {
  const fieldErrors: Record<string, string> = {};

  if (!error.invalid_params || !Array.isArray(error.invalid_params)) {
    return fieldErrors;
  }

  for (const param of error.invalid_params) {
    fieldErrors[param.name] = resolveFieldErrorMessage(param.code, param.reason);
  }

  return fieldErrors;
}

/**
 * Dispatches a standardized error notification via Mantine Notifications.
 */
export function notifyApiError(error: ProblemDetails): void {
  notifications.show({
    title: error.title || 'Error',
    message: getLocalizedErrorMessage(error),
    color: 'red',
  });
}

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

function createDefaultTranslator(): TranslationFn {
  return (key: string, options?: Record<string, unknown>) => {
    return i18n.t(key, options ?? {});
  };
}

function resolveFieldErrorMessage(code: string, fallbackReason: string): string {
  const paramKey = `errors.${code}`;
  if (i18n.exists(paramKey)) {
    return i18n.t(paramKey);
  }
  return fallbackReason;
}
