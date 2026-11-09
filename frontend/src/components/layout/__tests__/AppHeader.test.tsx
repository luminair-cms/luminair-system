import { AppShell } from '@mantine/core';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '@/auth/use-auth-store';
import { AppHeader } from '@/components/layout/AppHeader';
import { useUiStore } from '@/store/use-ui-store';
import { renderWithProviders } from '@/test-utils/render';

function renderHeader() {
  return renderWithProviders(
    <AppShell header={{ height: 60 }}>
      <AppHeader />
    </AppShell>
  );
}

describe('AppHeader', () => {
  beforeEach(() => {
    useUiStore.setState({
      sidebarCollapsed: false,
      activeLocale: 'en',
      colorScheme: 'light',
    });
    useAuthStore.getState().clearSession();
  });

  it('toggles sidebarCollapsed state in useUiStore when burger is clicked', async () => {
    const user = userEvent.setup();
    renderHeader();

    expect(useUiStore.getState().sidebarCollapsed).toBe(false);

    const burger = screen.getByRole('button', { name: /toggle navigation/i });
    await user.click(burger);

    expect(useUiStore.getState().sidebarCollapsed).toBe(true);

    await user.click(burger);
    expect(useUiStore.getState().sidebarCollapsed).toBe(false);
  });

  it('updates activeLocale in useUiStore when language is changed', async () => {
    const user = userEvent.setup();
    renderHeader();

    const localeSelect = screen.getByRole('combobox', { name: /select language/i });
    expect(localeSelect).toHaveValue('EN');

    await user.click(localeSelect);
    const ukOption = await screen.findByRole('option', { name: 'UK', hidden: true });
    await user.click(ukOption);

    expect(useUiStore.getState().activeLocale).toBe('uk');
  });

  it('toggles color scheme when theme button is clicked', async () => {
    const user = userEvent.setup();
    renderHeader();

    const themeButton = screen.getByRole('button', { name: /toggle color scheme/i });
    await user.click(themeButton);

    expect(useUiStore.getState().colorScheme).toBe('dark');

    await user.click(themeButton);
    expect(useUiStore.getState().colorScheme).toBe('light');
  });

  it('displays Guest badge when unauthenticated', () => {
    renderHeader();
    expect(screen.getByTestId('user-badge')).toHaveTextContent('Guest');
  });

  it('displays user email badge when session is active', () => {
    useAuthStore.getState().setSession(
      { access_token: 'fake-jwt-token' },
      { sub: 'usr-123', email: 'editor@luminair.dev', roles: ['editor'] }
    );

    renderHeader();
    expect(screen.getByTestId('user-badge')).toHaveTextContent('editor@luminair.dev');
  });
});
