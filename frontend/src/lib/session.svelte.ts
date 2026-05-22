// Session store: loads /api/me once on first access, exposes a $state for
// reactive consumers, and provides a logout helper that clears state.

import { fetchMe, logout as apiLogout, type CurrentUser } from './api';

interface SessionState {
  user: CurrentUser | null;
  loading: boolean;
  error: string | null;
}

const state = $state<SessionState>({ user: null, loading: true, error: null });
let loaded = false;

export function session() {
  if (!loaded) {
    loaded = true;
    void refresh();
  }
  return state;
}

export async function refresh(): Promise<void> {
  state.loading = true;
  state.error = null;
  try {
    state.user = await fetchMe();
  } catch (e) {
    state.error = e instanceof Error ? e.message : String(e);
    state.user = null;
  } finally {
    state.loading = false;
  }
}

export async function logout(): Promise<void> {
  await apiLogout();
  state.user = null;
}
