<script lang="ts">
  import { page } from '$app/state';
  import { mount } from '$lib/editor';
  import { submissions, problems, ApiError, type Submission, type Problem } from '$lib/api';
  import { type IoSpec } from '$lib/bytes';
  import CaseResultsPanel from '$lib/components/CaseResultsPanel.svelte';

  const id = $derived(page.params.id ?? '');
  const shareToken = $derived(page.url.searchParams.get('t') ?? undefined);

  let submission = $state<Submission | null>(null);
  let problem = $state<Problem | null>(null);
  let error = $state<string | null>(null);
  let editorHost = $state<HTMLDivElement | undefined>(undefined);

  const outputSpec = $derived((problem?.io_spec as IoSpec | undefined)?.output);

  $effect(() => {
    void (async () => {
      if (!id) return;
      try {
        const s = await submissions.get(id, shareToken);
        submission = s;
        if (s.problem_id) {
          problem = await problems.get(s.problem_id, shareToken);
        }
      } catch (e) {
        error = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
      }
    })();
  });

  $effect(() => {
    if (!submission || !editorHost) return;
    return mount(editorHost, {
      initialDoc: submission.source_code,
      readOnly: true
    });
  });
</script>

<main class="mx-auto flex min-h-screen max-w-5xl flex-col gap-6 px-6 py-10">
  {#if error}
    <p class="text-rose-400">{error}</p>
  {:else if submission}
    <header class="flex flex-col gap-1">
      <span class="font-mono text-xs text-zinc-500">submission {submission.id}</span>
      <div class="flex items-baseline gap-3">
        <h1 class="text-xl font-semibold tracking-tight text-zinc-100">
          {#if problem}
            <a class="hover:underline" href={`/p/${problem.id}${shareToken ? `?t=${shareToken}` : ''}`}
              >{problem.title}</a
            >
          {:else}
            (playground)
          {/if}
        </h1>
        <span class="font-mono text-xs text-zinc-500">on {submission.board}</span>
      </div>
      <div class="flex items-baseline gap-3 font-mono text-xs text-zinc-500">
        <span>by user #{submission.user_id}</span>
        <span>·</span>
        <span>{submission.created_at}</span>
        {#if submission.finished_at}
          <span>·</span>
          <span>finished {submission.finished_at}</span>
        {/if}
      </div>
    </header>

    <section class="flex flex-col gap-2">
      <span class="text-xs uppercase tracking-wider text-zinc-400">Source</span>
      <div
        bind:this={editorHost}
        class="h-96 overflow-hidden rounded border border-zinc-800 bg-zinc-900"
      ></div>
    </section>

    <section class="flex flex-col gap-2">
      <span class="text-xs uppercase tracking-wider text-zinc-400">Result</span>
      <div class="flex items-baseline gap-3 text-sm">
        <span class="font-mono text-zinc-300">status: {submission.status}</span>
      </div>
      <CaseResultsPanel {submission} {outputSpec} />
    </section>
  {:else}
    <p class="text-zinc-500">Loading…</p>
  {/if}
</main>
