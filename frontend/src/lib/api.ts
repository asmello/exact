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

// ---- Problems ------------------------------------------------------------

export type Visibility = 'private' | 'shared' | 'public';

export interface Problem {
  id: string;
  title: string;
  description_md: string;
  starter_code: string;
  io_spec: unknown;
  visibility: Visibility;
  share_token: string | null;
  default_timeout_ms: number;
  allowed_boards: string[];
  owner_id: number;
  created_at: string;
  updated_at: string;
}

export interface TestCase {
  id: number;
  problem_id: string;
  ord: number;
  name: string | null;
  /** base64-encoded input bytes */
  input: string;
  /** base64-encoded expected_output bytes; null = benchmark-only or redacted */
  expected_output: string | null;
  weight: number;
  hidden: boolean;
}

export interface CreateProblemBody {
  id: string;
  title: string;
  description_md: string;
  starter_code: string;
  io_spec: unknown;
  visibility: Visibility;
  default_timeout_ms: number;
  allowed_boards: string[];
}

export interface UpdateProblemBody {
  title?: string;
  description_md?: string;
  starter_code?: string;
  io_spec?: unknown;
  visibility?: Visibility;
  default_timeout_ms?: number;
  allowed_boards?: string[];
}

export interface CreateCaseBody {
  ord: number;
  name?: string | null;
  /** base64-encoded input bytes */
  input: string;
  /** base64-encoded expected_output bytes; omit or null for benchmark-only */
  expected_output?: string | null;
  weight?: number;
  hidden?: boolean;
}

async function json<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new ApiError(res.status, body || res.statusText);
  }
  if (res.status === 204) return undefined as unknown as T;
  return (await res.json()) as T;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
  }
}

export const problems = {
  list(): Promise<Problem[]> {
    return fetch('/api/problems', { credentials: 'same-origin' }).then(json<Problem[]>);
  },
  get(id: string, shareToken?: string): Promise<Problem> {
    const url = shareToken
      ? `/api/problems/${encodeURIComponent(id)}?t=${encodeURIComponent(shareToken)}`
      : `/api/problems/${encodeURIComponent(id)}`;
    return fetch(url, { credentials: 'same-origin' }).then(json<Problem>);
  },
  create(body: CreateProblemBody): Promise<Problem> {
    return fetch('/api/problems', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    }).then(json<Problem>);
  },
  update(id: string, body: UpdateProblemBody): Promise<Problem> {
    return fetch(`/api/problems/${encodeURIComponent(id)}`, {
      method: 'PUT',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    }).then(json<Problem>);
  },
  remove(id: string): Promise<void> {
    return fetch(`/api/problems/${encodeURIComponent(id)}`, {
      method: 'DELETE',
      credentials: 'same-origin'
    }).then(json<void>);
  }
};

export type Board = 'lm3s6965evb' | 'lpc1768' | 'stm32f429zi';

export interface Submission {
  id: string;
  user_id: number;
  problem_id: string | null;
  source_code: string;
  board: string;
  device_id: string | null;
  status: 'queued' | 'building' | 'running' | 'done' | 'failed';
  build_log: string | null;
  total_cycles: number | null;
  passed: number | null;
  total_cases: number | null;
  created_at: string;
  finished_at: string | null;
}

export interface CreateSubmissionBody {
  problem_id?: string | null;
  source_code: string;
  board: Board;
}

export const submissions = {
  create(body: CreateSubmissionBody): Promise<Submission> {
    return fetch('/api/submissions', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    }).then(json<Submission>);
  },
  get(id: string): Promise<Submission> {
    return fetch(`/api/submissions/${encodeURIComponent(id)}`, {
      credentials: 'same-origin'
    }).then(json<Submission>);
  }
};

export const cases = {
  list(problemId: string, shareToken?: string): Promise<TestCase[]> {
    const url = shareToken
      ? `/api/problems/${encodeURIComponent(problemId)}/cases?t=${encodeURIComponent(shareToken)}`
      : `/api/problems/${encodeURIComponent(problemId)}/cases`;
    return fetch(url, { credentials: 'same-origin' }).then(json<TestCase[]>);
  },
  create(problemId: string, body: CreateCaseBody): Promise<TestCase> {
    return fetch(`/api/problems/${encodeURIComponent(problemId)}/cases`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    }).then(json<TestCase>);
  },
  remove(problemId: string, ord: number): Promise<void> {
    return fetch(`/api/problems/${encodeURIComponent(problemId)}/cases/${ord}`, {
      method: 'DELETE',
      credentials: 'same-origin'
    }).then(json<void>);
  }
};
