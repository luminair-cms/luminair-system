import { create } from 'zustand';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export type OnboardingStatus = 'not_requested' | 'pending' | 'rejected' | 'approved' | null;

export interface AuthUser {
  sub: string;
  email?: string;
  name?: string;
  [key: string]: unknown;
}

export interface AuthTokens {
  access_token: string;
  id_token?: string;
}

export interface AuthState {
  user: AuthUser | null;
  accessToken: string | null;
  idToken: string | null;
  roles: string[];
  isAuthenticated: boolean;
  onboardingStatus: OnboardingStatus;
  setSession: (tokens: AuthTokens, claims: Record<string, unknown>) => void;
  setOnboardingStatus: (status: OnboardingStatus) => void;
  clearSession: () => void;
}

// ---------------------------------------------------------------------------
// Primary Export (Zustand Hook)
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  accessToken: null,
  idToken: null,
  roles: [],
  isAuthenticated: false,
  onboardingStatus: null,

  setSession: (tokens, claims) => {
    const roles = extractRoles(claims);
    const user: AuthUser = {
      sub: typeof claims.sub === 'string' ? claims.sub : '',
      email: typeof claims.email === 'string' ? claims.email : undefined,
      name: typeof claims.name === 'string' ? claims.name : undefined,
      ...claims,
    };

    set({
      user,
      accessToken: tokens.access_token,
      idToken: tokens.id_token ?? null,
      roles,
      isAuthenticated: true,
    });
  },

  setOnboardingStatus: (onboardingStatus) => set({ onboardingStatus }),

  clearSession: () => {
    set({
      user: null,
      accessToken: null,
      idToken: null,
      roles: [],
      isAuthenticated: false,
      onboardingStatus: null,
    });
  },
}));

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

function extractRoles(claims: Record<string, unknown>): string[] {
  if (Array.isArray(claims.roles)) {
    return claims.roles.filter((r): r is string => typeof r === 'string');
  }
  if (Array.isArray(claims.groups)) {
    return claims.groups.filter((g): g is string => typeof g === 'string');
  }
  return [];
}
