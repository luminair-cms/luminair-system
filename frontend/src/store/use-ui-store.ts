import { create } from 'zustand';
import { persist } from 'zustand/middleware';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export type ColorScheme = 'light' | 'dark' | 'auto';

export interface UiState {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  colorScheme: ColorScheme;
  setColorScheme: (scheme: ColorScheme) => void;
  activeLocale: string;
  setActiveLocale: (locale: string) => void;
}

// ---------------------------------------------------------------------------
// Primary Export (Persisted Zustand Hook)
// ---------------------------------------------------------------------------

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
      colorScheme: 'auto',
      setColorScheme: (colorScheme) => set({ colorScheme }),
      activeLocale: 'en',
      setActiveLocale: (activeLocale) => set({ activeLocale }),
    }),
    {
      name: 'luminair-ui-storage',
    }
  )
);
