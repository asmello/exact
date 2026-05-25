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
  status: 'queued' | 'building' | 'ready' | 'running' | 'done' | 'failed';
  build_log: string | null;
  total_cycles: number | null;
  passed: number | null;
  total_cases: number | null;
  created_at: string;
  finished_at: string | null;
  /** Populated by GET /api/submissions/:id (detail endpoint). */
  case_results?: CaseResult[];
}

export interface CaseResult {
  case_ord: number;
  status: string;
  exit_code: number | null;
  cycles: number | null;
  /** base64-encoded output bytes */
  output: string | null;
  passed: boolean | null;
  synthetic: boolean;
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

// ---- Devices + runners (admin) ------------------------------------------

export interface Device {
  id: string;
  board: string;
  cclk_hz: number;
  description: string | null;
  active: boolean;
  last_seen: string | null;
  synthetic: boolean;
  online: boolean;
}

export interface Runner {
  id: number;
  device_id: string;
  label: string;
  token_prefix: string;
  created_by: number;
  created_at: string;
  revoked_at: string | null;
  last_used_at: string | null;
}

export interface CreateDeviceBody {
  id: string;
  board: Board;
  cclk_hz: number;
  description?: string;
  synthetic?: boolean;
}

export interface CreateRunnerResponse {
  runner: Runner;
  token: string;
}

export const devices = {
  list(): Promise<Device[]> {
    return fetch('/api/devices', { credentials: 'same-origin' }).then(json<Device[]>);
  },
  create(body: CreateDeviceBody): Promise<Device> {
    return fetch('/api/admin/devices', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    }).then(json<Device>);
  }
};

export const runners = {
  list(): Promise<Runner[]> {
    return fetch('/api/admin/runners', { credentials: 'same-origin' }).then(json<Runner[]>);
  },
  create(device_id: string, label: string): Promise<CreateRunnerResponse> {
    return fetch('/api/admin/runners', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ device_id, label })
    }).then(json<CreateRunnerResponse>);
  },
  revoke(id: number): Promise<void> {
    return fetch(`/api/admin/runners/${id}`, {
      method: 'DELETE',
      credentials: 'same-origin'
    }).then(json<void>);
  }
};

// ---- Leaderboards --------------------------------------------------------

export interface LeaderboardEntry {
  rank: number;
  submission_id: string;
  user_id: number;
  github_login: string;
  avatar_url: string | null;
  total_cycles: number;
  finished_at: string;
  synthetic: boolean;
}

export interface LeaderboardResponse {
  problem_id: string;
  board: string;
  entries: LeaderboardEntry[];
  /** Viewer's best entry — present even when outside the top N. */
  you: LeaderboardEntry | null;
}

export const leaderboards = {
  get(
    problemId: string,
    board: string,
    opts?: { shareToken?: string; limit?: number }
  ): Promise<LeaderboardResponse> {
    const params = new URLSearchParams({ board });
    if (opts?.shareToken) params.set('t', opts.shareToken);
    if (opts?.limit) params.set('limit', String(opts.limit));
    return fetch(
      `/api/problems/${encodeURIComponent(problemId)}/leaderboard?${params.toString()}`,
      { credentials: 'same-origin' }
    ).then(json<LeaderboardResponse>);
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
