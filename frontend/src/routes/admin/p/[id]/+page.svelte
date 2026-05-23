<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import {
    problems,
    cases,
    ApiError,
    type Problem,
    type TestCase,
    type Visibility,
    type CreateCaseBody
  } from '$lib/api';
  import { hexToB64, b64ToHex } from '$lib/bytes';

  // /admin/p/new => creating; otherwise editing.
  const routeId = $derived(page.params.id ?? '');
  const creating = $derived(routeId === 'new');

  // Form state. For new problems, `id` is editable; for existing, it's locked.
  let id = $state('');
  let title = $state('');
  let description_md = $state('');
  let starter_code = $state('fn solve(n: u32) -> u64 {\n    (1..=n as u64).sum()\n}\n');
  let visibility = $state<Visibility>('private');
  let default_timeout_ms = $state(100);
  let allowed_boards = $state<string[]>(['lm3s6965evb']);
  let io_spec_text = $state('{\n  "input": "u32_le",\n  "output": "u64_le"\n}');

  let shareToken = $state<string | null>(null);
  let testCases = $state<TestCase[]>([]);
  let formError = $state<string | null>(null);
  let formStatus = $state<string | null>(null);
  let loading = $state(true);

  const boardChoices = ['lm3s6965evb', 'lpc1768', 'stm32f429zi'];

  $effect(() => {
    const idForLoad = routeId;
    if (idForLoad === 'new') {
      loading = false;
      return;
    }
    void (async () => {
      try {
        const [p, cs] = await Promise.all([problems.get(idForLoad), cases.list(idForLoad)]);
        loadProblem(p);
        testCases = cs;
      } catch (e) {
        formError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
      } finally {
        loading = false;
      }
    })();
  });

  function loadProblem(p: Problem) {
    id = p.id;
    title = p.title;
    description_md = p.description_md;
    starter_code = p.starter_code;
    visibility = p.visibility;
    default_timeout_ms = p.default_timeout_ms;
    allowed_boards = p.allowed_boards;
    io_spec_text = JSON.stringify(p.io_spec, null, 2);
    shareToken = p.share_token;
  }

  function toggleBoard(b: string) {
    allowed_boards = allowed_boards.includes(b)
      ? allowed_boards.filter((x) => x !== b)
      : [...allowed_boards, b];
  }

  async function save() {
    formError = null;
    formStatus = null;
    let io_spec: unknown;
    try {
      io_spec = JSON.parse(io_spec_text);
    } catch (e) {
      formError = `io_spec is not valid JSON: ${(e as Error).message}`;
      return;
    }
    try {
      if (creating) {
        const created = await problems.create({
          id,
          title,
          description_md,
          starter_code,
          io_spec,
          visibility,
          default_timeout_ms,
          allowed_boards
        });
        await goto(`/admin/p/${created.id}`, { replaceState: true });
      } else {
        const updated = await problems.update(routeId, {
          title,
          description_md,
          starter_code,
          io_spec,
          visibility,
          default_timeout_ms,
          allowed_boards
        });
        loadProblem(updated);
        formStatus = 'Saved.';
      }
    } catch (e) {
      formError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }

  async function destroy() {
    if (creating) return;
    if (!confirm(`Delete problem ${routeId}? This cannot be undone.`)) return;
    try {
      await problems.remove(routeId);
      await goto('/admin');
    } catch (e) {
      formError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }

  // Add-case form state
  let newCaseOrd = $state(0);
  let newCaseName = $state('');
  let newCaseInputHex = $state('');
  let newCaseExpectedHex = $state('');
  let newCaseWeight = $state(1.0);
  let newCaseHidden = $state(false);
  let caseError = $state<string | null>(null);

  $effect(() => {
    // Auto-advance ord to max+1 when cases load
    if (testCases.length > 0) {
      const maxOrd = Math.max(...testCases.map((c) => c.ord));
      newCaseOrd = maxOrd + 1;
    }
  });

  async function addCase() {
    caseError = null;
    try {
      const body: CreateCaseBody = {
        ord: newCaseOrd,
        name: newCaseName || null,
        input: hexToB64(newCaseInputHex),
        weight: newCaseWeight,
        hidden: newCaseHidden
      };
      if (newCaseExpectedHex.trim().length > 0) {
        body.expected_output = hexToB64(newCaseExpectedHex);
      }
      const created = await cases.create(routeId, body);
      testCases = [...testCases, created].sort((a, b) => a.ord - b.ord);
      newCaseOrd = created.ord + 1;
      newCaseName = '';
      newCaseInputHex = '';
      newCaseExpectedHex = '';
    } catch (e) {
      caseError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }

  async function removeCase(ord: number) {
    if (!confirm(`Delete case #${ord}?`)) return;
    try {
      await cases.remove(routeId, ord);
      testCases = testCases.filter((c) => c.ord !== ord);
    } catch (e) {
      caseError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }
</script>

<main class="mx-auto flex min-h-screen max-w-5xl flex-col gap-8 px-6 py-10">
  <header class="flex items-baseline justify-between">
    <h1 class="text-xl font-semibold tracking-tight text-zinc-100">
      {creating ? 'New problem' : `Edit · ${routeId}`}
    </h1>
    <div class="flex items-baseline gap-3">
      <a href="/admin" class="text-sm text-zinc-400 hover:text-zinc-200">← back</a>
      {#if !creating}
        <button
          type="button"
          onclick={destroy}
          class="rounded border border-rose-900 px-3 py-1 text-sm text-rose-400 hover:bg-rose-950"
          >delete</button
        >
      {/if}
    </div>
  </header>

  {#if loading}
    <p class="text-zinc-500">Loading…</p>
  {:else}
    <section class="flex flex-col gap-4">
      <label class="flex flex-col gap-1">
        <span class="text-xs uppercase tracking-wider text-zinc-400">slug (id)</span>
        <input
          type="text"
          bind:value={id}
          disabled={!creating}
          placeholder="sum-to-n"
          class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100 disabled:opacity-50"
        />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs uppercase tracking-wider text-zinc-400">title</span>
        <input
          type="text"
          bind:value={title}
          class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
        />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs uppercase tracking-wider text-zinc-400">description (markdown)</span>
        <textarea
          bind:value={description_md}
          rows="6"
          class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
        ></textarea>
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs uppercase tracking-wider text-zinc-400">starter code (Rust)</span>
        <textarea
          bind:value={starter_code}
          rows="8"
          class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
        ></textarea>
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-xs uppercase tracking-wider text-zinc-400">io_spec (JSON)</span>
        <textarea
          bind:value={io_spec_text}
          rows="4"
          class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
        ></textarea>
      </label>

      <div class="grid grid-cols-2 gap-4">
        <label class="flex flex-col gap-1">
          <span class="text-xs uppercase tracking-wider text-zinc-400">visibility</span>
          <select
            bind:value={visibility}
            class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
          >
            <option value="private">private</option>
            <option value="shared">shared</option>
            <option value="public">public</option>
          </select>
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-xs uppercase tracking-wider text-zinc-400">timeout (ms)</span>
          <input
            type="number"
            min="1"
            bind:value={default_timeout_ms}
            class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
          />
        </label>
      </div>

      <fieldset class="flex flex-col gap-2">
        <legend class="text-xs uppercase tracking-wider text-zinc-400">allowed boards</legend>
        <div class="flex gap-3">
          {#each boardChoices as b (b)}
            <label class="flex items-center gap-2 font-mono text-sm text-zinc-200">
              <input
                type="checkbox"
                checked={allowed_boards.includes(b)}
                onchange={() => toggleBoard(b)}
              />
              {b}
            </label>
          {/each}
        </div>
      </fieldset>

      {#if shareToken}
        <div class="flex flex-col gap-1">
          <span class="text-xs uppercase tracking-wider text-zinc-400">share URL</span>
          <code class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-xs text-zinc-300"
            >/p/{id}?t={shareToken}</code
          >
        </div>
      {/if}

      <div class="flex items-center gap-3">
        <button
          type="button"
          onclick={save}
          class="rounded border border-emerald-700 bg-emerald-900/30 px-4 py-2 text-sm text-emerald-300 hover:bg-emerald-900/50"
          >{creating ? 'Create' : 'Save'}</button
        >
        {#if formStatus}<span class="text-sm text-emerald-400">{formStatus}</span>{/if}
        {#if formError}<span class="text-sm text-rose-400">{formError}</span>{/if}
      </div>
    </section>

    {#if !creating}
      <section class="flex flex-col gap-4">
        <h2 class="text-lg font-semibold tracking-tight text-zinc-100">Test cases</h2>
        {#if testCases.length === 0}
          <p class="text-sm text-zinc-500">No cases yet.</p>
        {:else}
          <ul class="flex flex-col gap-2">
            {#each testCases as c (c.ord)}
              <li
                class="grid grid-cols-[auto_auto_1fr_auto] items-center gap-3 rounded border border-zinc-800 bg-zinc-900/40 px-3 py-2 font-mono text-xs"
              >
                <span class="text-zinc-500">#{c.ord}</span>
                <span class="text-zinc-200">{c.name ?? '(unnamed)'}</span>
                <span class="truncate text-zinc-400">
                  in: {b64ToHex(c.input) || '(empty)'}
                  {#if c.expected_output !== null}
                    · out: {b64ToHex(c.expected_output)}
                  {/if}
                </span>
                <div class="flex items-baseline gap-3 text-[11px]">
                  {#if c.hidden}<span
                      class="rounded border border-zinc-700 px-1.5 py-0.5 uppercase text-zinc-400"
                      >hidden</span
                    >{/if}
                  <span class="text-zinc-500">w {c.weight}</span>
                  <button
                    type="button"
                    onclick={() => removeCase(c.ord)}
                    class="text-rose-400 hover:text-rose-300">delete</button
                  >
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        <div class="flex flex-col gap-3 rounded border border-zinc-800 bg-zinc-900/30 p-4">
          <h3 class="text-sm font-semibold text-zinc-200">Add a case</h3>
          <div class="grid grid-cols-2 gap-3">
            <label class="flex flex-col gap-1">
              <span class="text-xs uppercase tracking-wider text-zinc-400">ord</span>
              <input
                type="number"
                min="0"
                bind:value={newCaseOrd}
                class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
              />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-xs uppercase tracking-wider text-zinc-400">name (optional)</span>
              <input
                type="text"
                bind:value={newCaseName}
                class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
              />
            </label>
          </div>
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">input (hex)</span>
            <textarea
              bind:value={newCaseInputHex}
              rows="2"
              placeholder="05000000"
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
            ></textarea>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400"
              >expected output (hex, optional)</span
            >
            <textarea
              bind:value={newCaseExpectedHex}
              rows="2"
              placeholder="0f00000000000000"
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
            ></textarea>
          </label>
          <div class="flex items-center gap-4">
            <label class="flex items-center gap-2 text-sm text-zinc-200">
              weight
              <input
                type="number"
                step="0.1"
                min="0"
                bind:value={newCaseWeight}
                class="w-20 rounded border border-zinc-800 bg-zinc-900 px-2 py-1 font-mono text-sm text-zinc-100"
              />
            </label>
            <label class="flex items-center gap-2 text-sm text-zinc-200">
              <input type="checkbox" bind:checked={newCaseHidden} />
              hidden
            </label>
            <button
              type="button"
              onclick={addCase}
              class="ml-auto rounded border border-zinc-700 px-3 py-1 text-sm text-zinc-200 hover:bg-zinc-800"
              >add</button
            >
          </div>
          {#if caseError}<p class="text-sm text-rose-400">{caseError}</p>{/if}
        </div>
      </section>
    {/if}
  {/if}
</main>
