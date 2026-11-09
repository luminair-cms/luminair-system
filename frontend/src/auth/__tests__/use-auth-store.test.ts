import { beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '../use-auth-store';

describe('useAuthStore', () => {
  beforeEach(() => {
    useAuthStore.getState().clearSession();
  });

  it('initializes with unauthenticated default state', () => {
    const state = useAuthStore.getState();
    expect(state.user).toBeNull();
    expect(state.accessToken).toBeNull();
    expect(state.idToken).toBeNull();
    expect(state.roles).toEqual([]);
    expect(state.isAuthenticated).toBe(false);
    expect(state.onboardingStatus).toBeNull();
  });

  it('sets user session and extracts roles from claims.roles', () => {
    useAuthStore.getState().setSession(
      { access_token: 'access-123', id_token: 'id-456' },
      {
        sub: 'usr-42',
        email: 'alice@example.com',
        name: 'Alice Editor',
        roles: ['admin', 'editor'],
      }
    );

    const state = useAuthStore.getState();
    expect(state.isAuthenticated).toBe(true);
    expect(state.accessToken).toBe('access-123');
    expect(state.idToken).toBe('id-456');
    expect(state.roles).toEqual(['admin', 'editor']);
    expect(state.user).toMatchObject({
      sub: 'usr-42',
      email: 'alice@example.com',
      name: 'Alice Editor',
    });
  });

  it('extracts roles from claims.groups if claims.roles is not present', () => {
    useAuthStore.getState().setSession(
      { access_token: 'access-123' },
      {
        sub: 'usr-43',
        groups: ['luminair-writers'],
      }
    );

    const state = useAuthStore.getState();
    expect(state.roles).toEqual(['luminair-writers']);
    expect(state.idToken).toBeNull();
  });

  it('updates onboarding status', () => {
    useAuthStore.getState().setOnboardingStatus('pending');
    expect(useAuthStore.getState().onboardingStatus).toBe('pending');

    useAuthStore.getState().setOnboardingStatus('rejected');
    expect(useAuthStore.getState().onboardingStatus).toBe('rejected');
  });

  it('clears session and restores defaults', () => {
    useAuthStore.getState().setSession(
      { access_token: 'access-123' },
      { sub: 'usr-42', roles: ['admin'] }
    );
    useAuthStore.getState().setOnboardingStatus('approved');

    useAuthStore.getState().clearSession();

    const state = useAuthStore.getState();
    expect(state.user).toBeNull();
    expect(state.accessToken).toBeNull();
    expect(state.roles).toEqual([]);
    expect(state.isAuthenticated).toBe(false);
    expect(state.onboardingStatus).toBeNull();
  });
});
