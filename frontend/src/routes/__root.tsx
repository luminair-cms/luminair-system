import type { ReactElement } from 'react';
import { createRootRoute, Outlet } from '@tanstack/react-router';
import { AppShellLayout } from '@/components/layout/AppShellLayout';

// ---------------------------------------------------------------------------
// Primary Export (Root Route Definition)
// ---------------------------------------------------------------------------

export const Route = createRootRoute({
  component: RootLayout,
});

// ---------------------------------------------------------------------------
// Internal Helpers & Sub-Components
// ---------------------------------------------------------------------------

function RootLayout(): ReactElement {
  return (
    <AppShellLayout>
      <Outlet />
    </AppShellLayout>
  );
}
