import type { ReactElement } from 'react';
import {
  ActionIcon,
  AppShell,
  Badge,
  Burger,
  Group,
  Select,
  Title,
  useComputedColorScheme,
  useMantineColorScheme,
} from '@mantine/core';
import { IconMoon, IconSun } from '@tabler/icons-react';
import { useAuthStore } from '@/auth/use-auth-store';
import { useUiStore } from '@/store/use-ui-store';

// ---------------------------------------------------------------------------
// Primary Export
// ---------------------------------------------------------------------------

export function AppHeader(): ReactElement {
  return (
    <AppShell.Header px="md">
      <Group h="100%" justify="space-between">
        <HeaderBrand />
        <HeaderControls />
      </Group>
    </AppShell.Header>
  );
}

// ---------------------------------------------------------------------------
// Internal Helpers & Sub-Components
// ---------------------------------------------------------------------------

function HeaderBrand(): ReactElement {
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);

  return (
    <Group gap="sm">
      <Burger
        opened={!sidebarCollapsed}
        onClick={toggleSidebar}
        size="sm"
        aria-label="Toggle navigation"
      />
      <Title order={4}>Luminair Studio</Title>
    </Group>
  );
}

function HeaderControls(): ReactElement {
  return (
    <Group gap="sm">
      <LocaleSelector />
      <ColorSchemeToggle />
      <UserStatusBadge />
    </Group>
  );
}

function LocaleSelector(): ReactElement {
  const activeLocale = useUiStore((s) => s.activeLocale);
  const setActiveLocale = useUiStore((s) => s.setActiveLocale);

  return (
    <Select
      size="xs"
      w={85}
      value={activeLocale}
      onChange={(val) => val && setActiveLocale(val)}
      data={LOCALE_OPTIONS}
      allowDeselect={false}
      aria-label="Select language"
    />
  );
}

function ColorSchemeToggle(): ReactElement {
  const { setColorScheme } = useMantineColorScheme();
  const computedColorScheme = useComputedColorScheme('light', { getInitialValueInEffect: true });
  const setUiStoreScheme = useUiStore((s) => s.setColorScheme);

  const toggleScheme = () => {
    const nextScheme = computedColorScheme === 'dark' ? 'light' : 'dark';
    setColorScheme(nextScheme);
    setUiStoreScheme(nextScheme);
  };

  return (
    <ActionIcon
      variant="default"
      size="md"
      onClick={toggleScheme}
      aria-label="Toggle color scheme"
    >
      {computedColorScheme === 'dark' ? <IconSun size={18} /> : <IconMoon size={18} />}
    </ActionIcon>
  );
}

function UserStatusBadge(): ReactElement {
  const user = useAuthStore((s) => s.user);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);

  if (isAuthenticated && user) {
    return (
      <Badge variant="light" color="blue" data-testid="user-badge">
        {user.email ?? user.name ?? user.sub}
      </Badge>
    );
  }

  return (
    <Badge variant="outline" color="gray" data-testid="user-badge">
      Guest
    </Badge>
  );
}

// ---------------------------------------------------------------------------
// Constants & Static Metadata
// ---------------------------------------------------------------------------

const LOCALE_OPTIONS = [
  { value: 'en', label: 'EN' },
  { value: 'uk', label: 'UK' },
  { value: 'ru', label: 'RU' },
];
