import { screen } from '@testing-library/react';
import { createMemoryHistory, createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router';
import { describe, expect, it } from 'vitest';
import { BreadcrumbBar } from '@/components/layout/BreadcrumbBar';
import { renderWithProviders } from '@/test-utils/render';

function createTestRouter(ui: React.ReactElement) {
  const rootRoute = createRootRoute({
    component: () => ui,
  });
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
  });
  const routeTree = rootRoute.addChildren([indexRoute]);
  const history = createMemoryHistory({ initialEntries: ['/'] });
  return createRouter({ routeTree, history });
}

describe('BreadcrumbBar', () => {
  it('renders breadcrumb links and leaf item correctly', async () => {
    const items = [
      { label: 'Dashboard', href: '/' },
      { label: 'Articles', href: '/content/article' },
      { label: 'Edit Article' },
    ];

    const router = createTestRouter(<BreadcrumbBar items={items} />);
    renderWithProviders(<RouterProvider router={router} />);

    expect(await screen.findByRole('link', { name: 'Dashboard' })).toBeInTheDocument();
    expect(await screen.findByRole('link', { name: 'Articles' })).toBeInTheDocument();
    expect(await screen.findByText('Edit Article')).toBeInTheDocument();
  });
});
