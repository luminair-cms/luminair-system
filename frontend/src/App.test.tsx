import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { App } from './App';
import { createTestQueryClient } from './test-utils/render';

describe('App Integration', () => {
  it('renders dashboard route with system configuration from MSW', async () => {
    const queryClient = createTestQueryClient();

    render(<App queryClient={queryClient} />);

    // Dashboard header
    expect(await screen.findByText('Luminair Admin Studio')).toBeInTheDocument();

    // System config card populated by MSW mock
    expect(await screen.findByText('System Configuration')).toBeInTheDocument();
    expect(await screen.findByText('cfg-default-001')).toBeInTheDocument();
    expect(await screen.findByText('uk')).toBeInTheDocument();
    const enBadges = await screen.findAllByText('en');
    expect(enBadges.length).toBeGreaterThanOrEqual(1);
  });
});
