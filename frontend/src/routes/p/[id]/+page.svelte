<script lang="ts">
  import { page } from '$app/state';
  import { mount } from '$lib/editor';
  import { problems, cases, ApiError, type Problem, type TestCase } from '$lib/api';
  import { b64ToHex } from '$lib/bytes';

  const id = $derived(page.params.id ?? '');
  const shareToken = $derived(page.url.searchParams.get('t') ?? undefined);

  let problem = $state<Problem | null>(null);
  let testCases = $state<TestCase[]>([]);
  let error = $state<string | null>(null);
  let editorHost = $state<HTMLDivElement | undefined>(undefined);

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
      onChange: () => {}
    });
  });
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
      <span class="text-xs uppercase tracking-wider text-zinc-400">Editor (preview)</span>
      <div
        bind:this={editorHost}
        class="h-96 overflow-hidden rounded border border-zinc-800 bg-zinc-900"
      ></div>
      <p class="text-xs text-zinc-500">
        Submission flow lands in step 7. For now this is a read/edit-on-screen preview of the
        starter code.
      </p>
    </section>

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
