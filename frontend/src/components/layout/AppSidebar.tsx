import type { ReactElement } from 'react';
import { AppShell, Divider, NavLink, Stack, Text } from '@mantine/core';
import {
  IconDatabase,
  IconFileDescription,
  IconLayoutDashboard,
  IconUserCheck,
} from '@tabler/icons-react';
import { Link, useRouterState } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

// ---------------------------------------------------------------------------
// Primary Export
// ---------------------------------------------------------------------------

export function AppSidebar(): ReactElement {
  return (
    <AppShell.Navbar p="md">
      <Stack gap="xs">
        <SidebarNavSection />
        <Divider my="xs" />
        <DynamicContentTypesSection />
      </Stack>
    </AppShell.Navbar>
  );
}

// ---------------------------------------------------------------------------
// Internal Helpers & Sub-Components
// ---------------------------------------------------------------------------

function SidebarNavSection(): ReactElement {
  const { t } = useTranslation();
  const routerState = useRouterState();
  const currentPath = routerState.location.pathname;

  return (
    <Stack gap={4}>
      <NavLink
        component={Link}
        to="/"
        label={t('nav.dashboard', 'Dashboard')}
        leftSection={<IconLayoutDashboard size={18} />}
        active={currentPath === '/'}
      />
      <NavLink
        component={Link}
        to="/schema"
        label={t('nav.schema', 'Schema Inspector')}
        leftSection={<IconDatabase size={18} />}
        active={currentPath.startsWith('/schema')}
      />
      <NavLink
        component={Link}
        to="/admin/access-requests"
        label={t('nav.accessRequests', 'Access Requests')}
        leftSection={<IconUserCheck size={18} />}
        active={currentPath.startsWith('/admin/access-requests')}
      />
    </Stack>
  );
}

function DynamicContentTypesSection(): ReactElement {
  return (
    <NavLink
      label="Content Types"
      leftSection={<IconFileDescription size={18} />}
      defaultOpened
      childrenOffset={28}
    >
      <Text c="dimmed" size="xs" py="xs" px="xs">
        Dynamic content types will load here in Phase 3.
      </Text>
    </NavLink>
  );
}
