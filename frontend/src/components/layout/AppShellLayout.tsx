import type { ReactElement, ReactNode } from 'react';
import { AppShell } from '@mantine/core';
import { useUiStore } from '@/store/use-ui-store';
import { AppHeader } from './AppHeader';
import { AppSidebar } from './AppSidebar';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface AppShellLayoutProps {
  children: ReactNode;
}

// ---------------------------------------------------------------------------
// Primary Export
// ---------------------------------------------------------------------------

export function AppShellLayout({ children }: AppShellLayoutProps): ReactElement {
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);

  return (
    <AppShell
      header={{ height: 60 }}
      navbar={{
        width: 260,
        breakpoint: 'sm',
        collapsed: { mobile: sidebarCollapsed, desktop: sidebarCollapsed },
      }}
      padding="md"
    >
      <AppHeader />
      <AppSidebar />
      <AppShell.Main>{children}</AppShell.Main>
    </AppShell>
  );
}
