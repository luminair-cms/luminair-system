import { screen } from '@testing-library/react';
import { createMemoryHistory, createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router';
import { describe, expect, it } from 'vitest';
import { AppShellLayout } from '@/components/layout/AppShellLayout';
import { renderWithProviders } from '@/test-utils/render';

function createTestRouterWithLayout() {
  const rootRoute = createRootRoute({
    component: () => (
      <AppShellLayout>
        <div data-testid="test-content">Dashboard Content</div>
      </AppShellLayout>
    ),
  });
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
  });
  const routeTree = rootRoute.addChildren([indexRoute]);
  const history = createMemoryHistory({ initialEntries: ['/'] });
  return createRouter({ routeTree, history });
}

describe('AppShellLayout', () => {
  it('renders header brand, sidebar navigation links, and main content', async () => {
    const router = createTestRouterWithLayout();
    renderWithProviders(<RouterProvider router={router} />);

    // Brand in Header
    expect(await screen.findByText('Luminair Studio')).toBeInTheDocument();

    // Nav Links in Sidebar
    expect(await screen.findByRole('link', { name: /dashboard/i })).toBeInTheDocument();
    expect(await screen.findByRole('link', { name: /schema/i })).toBeInTheDocument();
    expect(await screen.findByRole('link', { name: /access requests/i })).toBeInTheDocument();
    expect(await screen.findByText('Content Types')).toBeInTheDocument();
    expect(
      await screen.findByText('Dynamic content types will load here in Phase 3.')
    ).toBeInTheDocument();

    // Main Children Content
    expect(await screen.findByTestId('test-content')).toBeInTheDocument();
  });
});
