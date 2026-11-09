import { beforeEach, describe, expect, it } from 'vitest';
import { useUiStore } from '../use-ui-store';

describe('useUiStore', () => {
  beforeEach(() => {
    localStorage.clear();
    useUiStore.setState({
      sidebarCollapsed: false,
      colorScheme: 'auto',
      activeLocale: 'en',
    });
  });

  it('initializes with default UI state', () => {
    const state = useUiStore.getState();
    expect(state.sidebarCollapsed).toBe(false);
    expect(state.colorScheme).toBe('auto');
    expect(state.activeLocale).toBe('en');
  });

  it('toggles sidebar collapsed state', () => {
    useUiStore.getState().toggleSidebar();
    expect(useUiStore.getState().sidebarCollapsed).toBe(true);

    useUiStore.getState().toggleSidebar();
    expect(useUiStore.getState().sidebarCollapsed).toBe(false);
  });

  it('updates color scheme preference', () => {
    useUiStore.getState().setColorScheme('dark');
    expect(useUiStore.getState().colorScheme).toBe('dark');

    useUiStore.getState().setColorScheme('light');
    expect(useUiStore.getState().colorScheme).toBe('light');
  });

  it('updates active locale', () => {
    useUiStore.getState().setActiveLocale('uk');
    expect(useUiStore.getState().activeLocale).toBe('uk');
  });
});
