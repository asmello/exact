<script lang="ts">
  import { page } from '$app/state';
  import { mount } from '$lib/editor';
  import {
    problems,
    cases,
    submissions,
    ApiError,
    type Problem,
    type TestCase,
    type Submission,
    type Board
  } from '$lib/api';
  import { b64ToHex } from '$lib/bytes';

  const id = $derived(page.params.id ?? '');
  const shareToken = $derived(page.url.searchParams.get('t') ?? undefined);

  let problem = $state<Problem | null>(null);
  let testCases = $state<TestCase[]>([]);
  let error = $state<string | null>(null);
  let editorHost = $state<HTMLDivElement | undefined>(undefined);

  let source = $state('');
  let board = $state<Board>('lm3s6965evb');
  let submission = $state<Submission | null>(null);
  let submitting = $state(false);
  let submitError = $state<string | null>(null);

  $effect(() => {
    void (async () => {
      if (!id) return;
      try {
        const [p, cs] = await Promise.all([
          problems.get(id, shareToken),
          cases.list(id, shareToken)
        ]);
        problem = p;
        testCases = cs;
        source = p.starter_code;
        if (p.allowed_boards.length > 0) {
          board = p.allowed_boards[0] as Board;
        }
      } catch (e) {
        error = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
      }
    })();
  });

  // Once we have starter_code AND the host element, mount the editor.
  $effect(() => {
    if (!problem || !editorHost) return;
    return mount(editorHost, {
      initialDoc: problem.starter_code,
      onChange: (doc) => {
        source = doc;
      }
    });
  });

  async function submit() {
    if (!problem || submitting) return;
    submitError = null;
    submitting = true;
    try {
      const created = await submissions.create({
        problem_id: problem.id,
        source_code: source,
        board
      });
      submission = await submissions.get(created.id);
      // Poll until terminal status. Step 7 will replace with SSE.
      while (submission && submission.status !== 'done' && submission.status !== 'failed') {
        await new Promise((r) => setTimeout(r, 500));
        submission = await submissions.get(created.id);
      }
    } catch (e) {
      submitError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<main class="mx-auto flex min-h-screen max-w-5xl flex-col gap-6 px-6 py-10">
  {#if error}
    <p class="text-rose-400">{error}</p>
  {:else if problem}
    <header class="flex items-baseline justify-between">
      <div class="flex items-baseline gap-3">
        <span class="font-mono text-xs text-zinc-500">{problem.id}</span>
        <h1 class="text-xl font-semibold tracking-tight text-zinc-100">{problem.title}</h1>
      </div>
      <div class="flex items-baseline gap-2 font-mono text-xs text-zinc-500">
        <span>{problem.allowed_boards.join(', ')}</span>
        <span>·</span>
        <span>{problem.default_timeout_ms}ms</span>
      </div>
    </header>

    <section class="flex flex-col gap-2">
      <span class="text-xs uppercase tracking-wider text-zinc-400">Description</span>
      <div
        class="whitespace-pre-wrap rounded border border-zinc-800 bg-zinc-900/40 px-4 py-3 font-mono text-sm text-zinc-200"
      >{problem.description_md}</div>
    </section>

    <section class="flex flex-col gap-2">
      <div class="flex items-center justify-between">
        <span class="text-xs uppercase tracking-wider text-zinc-400">Editor</span>
        <div class="flex items-center gap-3 text-sm">
          <label class="flex items-center gap-2 text-zinc-300">
            board
            <select
              bind:value={board}
              class="rounded border border-zinc-800 bg-zinc-900 px-2 py-1 font-mono text-xs text-zinc-100"
            >
              {#each problem.allowed_boards as b (b)}
                <option value={b}>{b}</option>
              {/each}
            </select>
          </label>
          <button
            type="button"
            onclick={submit}
            disabled={submitting}
            class="rounded border border-emerald-700 bg-emerald-900/30 px-3 py-1 text-emerald-300 hover:bg-emerald-900/50 disabled:opacity-50"
          >
            {submitting ? 'building…' : 'submit'}
          </button>
        </div>
      </div>
      <div
        bind:this={editorHost}
        class="h-96 overflow-hidden rounded border border-zinc-800 bg-zinc-900"
      ></div>
    </section>

    {#if submitError}
      <p class="text-sm text-rose-400">{submitError}</p>
    {/if}

    {#if submission}
      <section class="flex flex-col gap-2">
        <span class="text-xs uppercase tracking-wider text-zinc-400">
          Submission {submission.id.slice(0, 8)}…
        </span>
        <div class="flex items-baseline gap-3 text-sm">
          <span class="font-mono text-zinc-300">status: {submission.status}</span>
          {#if submission.finished_at}
            <span class="text-zinc-500">finished {submission.finished_at}</span>
          {/if}
        </div>
        {#if submission.status === 'failed' && submission.build_log}
          <pre
            class="max-h-96 overflow-auto rounded border border-rose-900 bg-rose-950/30 px-3 py-2 font-mono text-xs text-rose-200 whitespace-pre-wrap">{submission.build_log}</pre>
        {/if}

        {#if submission.case_results && submission.case_results.length > 0}
          <ul class="flex flex-col gap-1 font-mono text-xs">
            {#each submission.case_results as r (r.case_ord)}
              <li
                class="grid grid-cols-[auto_auto_1fr_auto_auto] items-center gap-3 rounded border border-zinc-800 bg-zinc-900/40 px-3 py-2"
              >
                <span class="text-zinc-500">#{r.case_ord}</span>
                {#if r.status === 'OK'}
                  {#if r.passed === false}
                    <span class="rounded border border-rose-800 bg-rose-950/40 px-1.5 py-0.5 text-[10px] uppercase text-rose-400"
                      >WRONG</span
                    >
                  {:else if r.passed === true}
                    <span class="rounded border border-emerald-800 bg-emerald-950/40 px-1.5 py-0.5 text-[10px] uppercase text-emerald-400"
                      >PASS</span
                    >
                  {:else}
                    <span class="rounded border border-zinc-700 px-1.5 py-0.5 text-[10px] uppercase text-zinc-400"
                      >OK</span
                    >
                  {/if}
                {:else}
                  <span class="rounded border border-rose-800 bg-rose-950/40 px-1.5 py-0.5 text-[10px] uppercase text-rose-400"
                    >{r.status}</span
                  >
                {/if}
                <span class="truncate text-zinc-400">
                  {#if r.output !== null}
                    out: {b64ToHex(r.output) || '(empty)'}
                  {/if}
                </span>
                <span class="text-zinc-300">
                  {r.cycles !== null ? `${r.cycles.toLocaleString()} cy` : '—'}
                </span>
                {#if r.synthetic}
                  <span class="rounded border border-amber-800 bg-amber-950/30 px-1.5 py-0.5 text-[10px] uppercase text-amber-400"
                    >synth</span
                  >
                {:else}
                  <span></span>
                {/if}
              </li>
            {/each}
          </ul>
          {#if submission.total_cycles !== null && submission.status === 'done'}
            <p class="text-sm text-zinc-300">
              Total {submission.total_cycles.toLocaleString()} cycles · {submission.passed}/{submission.total_cases}
              passed
            </p>
          {/if}
        {/if}
      </section>
    {/if}

    <section class="flex flex-col gap-2">
      <span class="text-xs uppercase tracking-wider text-zinc-400">Visible test cases</span>
      {#if testCases.length === 0}
        <p class="text-sm text-zinc-500">No visible cases.</p>
      {:else}
        <ul class="flex flex-col gap-1 font-mono text-xs">
          {#each testCases as c (c.ord)}
            <li class="rounded border border-zinc-800 bg-zinc-900/40 px-3 py-2">
              <div class="flex items-baseline gap-3">
                <span class="text-zinc-500">#{c.ord}</span>
                <span class="text-zinc-200">{c.name ?? '(unnamed)'}</span>
                {#if c.hidden}
                  <span class="rounded border border-zinc-700 px-1.5 py-0.5 text-[10px] uppercase text-zinc-400"
                    >hidden</span
                  >
                {/if}
                <span class="ml-auto text-zinc-500">weight {c.weight}</span>
              </div>
              <div class="mt-1 text-zinc-400">in: {b64ToHex(c.input) || '(empty)'}</div>
              {#if c.expected_output !== null}
                <div class="text-zinc-400">out: {b64ToHex(c.expected_output)}</div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else}
    <p class="text-zinc-500">Loading…</p>
  {/if}
</main>
