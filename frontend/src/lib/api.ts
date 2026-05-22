// Typed client for the exact backend. All routes run through Vite's dev proxy
// (or the same origin in prod), so relative URLs are correct everywhere.

export interface CurrentUser {
  id: number;
  github_id: number;
  github_login: string;
  avatar_url: string | null;
  is_admin: boolean;
  created_at: string;
}

export async function fetchMe(): Promise<CurrentUser | null> {
  const res = await fetch('/api/me', { credentials: 'same-origin' });
  if (res.status === 401) return null;
  if (!res.ok) throw new Error(`/api/me ${res.status}`);
  return (await res.json()) as CurrentUser;
}

export async function logout(): Promise<void> {
  const res = await fetch('/auth/logout', { method: 'POST', credentials: 'same-origin' });
  if (!res.ok && res.status !== 204) throw new Error(`logout ${res.status}`);
}
