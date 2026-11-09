import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ProblemDetails } from '@/api/errors';
import { ErrorAlert } from '@/components/common/ErrorAlert';
import { renderWithProviders } from '@/test-utils/render';

describe('ErrorAlert', () => {
  const sampleError: ProblemDetails = {
    type: 'urn:luminair:error:already_published',
    title: 'Conflict',
    status: 409,
    code: 'already_published',
    detail: 'Raw backend developer detail that should not be shown',
  };

  it('renders localized RFC 9457 error message instead of raw backend detail', () => {
    renderWithProviders(<ErrorAlert error={sampleError} />);

    expect(
      screen.getByText('This document is already published with no pending changes.')
    ).toBeInTheDocument();
    expect(
      screen.queryByText('Raw backend developer detail that should not be shown')
    ).not.toBeInTheDocument();
  });

  it('interpolates error details into localized message', () => {
    const conflictError: ProblemDetails = {
      type: 'urn:luminair:error:draft_has_pending_edits',
      title: 'Conflict',
      status: 409,
      code: 'draft_has_pending_edits',
      detail: 'Backend raw conflict detail',
      details: {
        target_revision: 2,
        draft_version: 5,
      },
    };

    renderWithProviders(<ErrorAlert error={conflictError} />);

    expect(
      screen.getByText(
        'Restoring revision 2 will discard your uncommitted draft changes (version 5). Do you wish to proceed?'
      )
    ).toBeInTheDocument();
  });

  it('renders custom title when provided', () => {
    renderWithProviders(<ErrorAlert error={sampleError} title="Custom Error Header" />);
    expect(screen.getByText('Custom Error Header')).toBeInTheDocument();
  });

  it('renders retry button and triggers callback on click', async () => {
    const user = userEvent.setup();
    const handleRetry = vi.fn();

    renderWithProviders(<ErrorAlert error={sampleError} onRetry={handleRetry} />);

    const retryButton = screen.getByRole('button', { name: /retry/i });
    expect(retryButton).toBeInTheDocument();

    await user.click(retryButton);
    expect(handleRetry).toHaveBeenCalledTimes(1);
  });

  it('does not render retry button when onRetry is omitted', () => {
    renderWithProviders(<ErrorAlert error={sampleError} />);
    expect(screen.queryByRole('button', { name: /retry/i })).not.toBeInTheDocument();
  });
});
