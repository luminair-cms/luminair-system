import { notifications } from '@mantine/notifications';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProblemDetails } from '@/api/errors';
import { i18n } from '../config';
import { getLocalizedErrorMessage, mapValidationErrorsToForm, notifyApiError } from '../error-mapper';

describe('error-mapper', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('en');
    vi.restoreAllMocks();
  });

  describe('getLocalizedErrorMessage', () => {
    it('translates known error code without interpolation', () => {
      const error: ProblemDetails = {
        type: 'urn:luminair:error:already_published',
        title: 'Conflict',
        status: 409,
        code: 'already_published',
        detail: 'Raw backend debug message for developers',
      };

      const message = getLocalizedErrorMessage(error);
      expect(message).toBe('This document is already published with no pending changes.');
      expect(message).not.toBe(error.detail);
    });

    it('interpolates contextual details into translated error string', () => {
      const error: ProblemDetails = {
        type: 'urn:luminair:error:draft_has_pending_edits',
        title: 'Conflict',
        status: 409,
        code: 'draft_has_pending_edits',
        detail: 'Restoring snapshot would overwrite pending working draft edits',
        details: {
          target_revision: 2,
          draft_version: 5,
        },
      };

      const message = getLocalizedErrorMessage(error);
      expect(message).toContain('revision 2');
      expect(message).toContain('version 5');
    });

    it('translates correctly when active language changes to Ukrainian', async () => {
      await i18n.changeLanguage('uk');

      const error: ProblemDetails = {
        type: 'urn:luminair:error:already_published',
        title: 'Conflict',
        status: 409,
        code: 'already_published',
        detail: 'Backend raw detail',
      };

      const message = getLocalizedErrorMessage(error);
      expect(message).toBe('Цей документ уже опублікований і не має очікуваних змін.');
    });

    it('falls back to errors.unknown_error when error code is unknown', () => {
      const error: ProblemDetails = {
        type: 'urn:luminair:error:some_unregistered_code',
        title: 'Unexpected',
        status: 500,
        code: 'some_unregistered_code',
        detail: 'Very detailed technical stacktrace in English',
      };

      const message = getLocalizedErrorMessage(error);
      // Conforms to .ai/rules/frontend-react.md: Never display raw backend detail to users
      expect(message).toBe('An unexpected error occurred.');
      expect(message).not.toBe(error.detail);
    });

    it('accepts a custom translation function override', () => {
      const error: ProblemDetails = {
        type: 'urn:luminair:error:already_published',
        title: 'Conflict',
        status: 409,
        code: 'already_published',
        detail: 'Raw backend detail',
      };

      const customTranslator = vi.fn().mockReturnValue('Custom localized message');
      const message = getLocalizedErrorMessage(error, customTranslator);

      expect(customTranslator).toHaveBeenCalledWith('errors.already_published', undefined);
      expect(message).toBe('Custom localized message');
    });
  });

  describe('mapValidationErrorsToForm', () => {
    it('maps invalid_params into a key-value record with translated field error', () => {
      const error: ProblemDetails = {
        type: 'urn:luminair:error:validation_failed',
        title: 'Unprocessable Entity',
        status: 422,
        code: 'validation_failed',
        detail: 'Validation failed',
        invalid_params: [
          {
            name: 'title',
            reason: 'must not be empty',
            code: 'required_field_missing',
          },
          {
            name: 'custom_field',
            reason: 'custom constraint violated',
            code: 'non_existent_code',
          },
        ],
      };

      const formErrors = mapValidationErrorsToForm(error);

      expect(formErrors.title).toBe('Field is required and cannot be empty.');
      expect(formErrors.custom_field).toBe('custom constraint violated');
    });

    it('returns empty record when invalid_params is empty or undefined', () => {
      const errorWithoutParams: ProblemDetails = {
        type: 'urn:luminair:error:internal_error',
        title: 'Internal Server Error',
        status: 500,
        code: 'internal_error',
        detail: 'Something went wrong',
      };

      expect(mapValidationErrorsToForm(errorWithoutParams)).toEqual({});
    });
  });

  describe('notifyApiError', () => {
    it('displays red error notification via Mantine notifications.show', () => {
      const showSpy = vi.spyOn(notifications, 'show').mockImplementation(() => '');

      const error: ProblemDetails = {
        type: 'urn:luminair:error:already_published',
        title: 'Conflict Notice',
        status: 409,
        code: 'already_published',
        detail: 'Raw backend detail',
      };

      notifyApiError(error);

      expect(showSpy).toHaveBeenCalledWith({
        title: 'Conflict Notice',
        message: 'This document is already published with no pending changes.',
        color: 'red',
      });
    });
  });
});
