import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { StatusBadge } from '@/components/common/StatusBadge';
import { renderWithProviders } from '@/test-utils/render';

describe('StatusBadge', () => {
  it('renders draft status with localized label', () => {
    renderWithProviders(<StatusBadge status="draft" />);
    expect(screen.getByText('Draft')).toBeInTheDocument();
  });

  it('renders published status with localized label', () => {
    renderWithProviders(<StatusBadge status="published" />);
    expect(screen.getByText('Published')).toBeInTheDocument();
  });

  it('renders modified status with localized label', () => {
    renderWithProviders(<StatusBadge status="modified" />);
    expect(screen.getByText('Modified')).toBeInTheDocument();
  });

  it('renders unpublished status with localized label', () => {
    renderWithProviders(<StatusBadge status="unpublished" />);
    expect(screen.getByText('Unpublished')).toBeInTheDocument();
  });

  it('renders revision badge when revision is provided', () => {
    renderWithProviders(<StatusBadge status="published" revision={2} />);
    expect(screen.getByText('Published')).toBeInTheDocument();
    expect(screen.getByText('r2')).toBeInTheDocument();
  });

  it('renders draftVersion badge when draftVersion is provided', () => {
    renderWithProviders(<StatusBadge status="draft" draftVersion={3} />);
    expect(screen.getByText('Draft')).toBeInTheDocument();
    expect(screen.getByText('v3')).toBeInTheDocument();
  });

  it('renders both revision and draftVersion badges simultaneously', () => {
    renderWithProviders(<StatusBadge status="modified" revision={4} draftVersion={5} />);
    expect(screen.getByText('Modified')).toBeInTheDocument();
    expect(screen.getByText('r4')).toBeInTheDocument();
    expect(screen.getByText('v5')).toBeInTheDocument();
  });
});
