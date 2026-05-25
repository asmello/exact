<script lang="ts">
  import { problems, submissions, ApiError, type Problem, type UserBestRow } from '$lib/api';
  import { session } from '$lib/session.svelte';

  const me = session();

  let list = $state<Problem[]>([]);
  let bestByProblem = $state<Map<string, UserBestRow[]>>(new Map());
  let loading = $state(true);
  let error = $state<string | null>(null);

  $effect(() => {
    void (async () => {
      try {
        list = await problems.list();
      } catch (e) {
        error = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
      } finally {
        loading = false;
      }
    })();
  });

  // Fetch viewer's bests in parallel with the problem list — they don't
  // depend on each other; the chip render is keyed by problem.id.
  $effect(() => {
    if (!me.user) {
      bestByProblem = new Map();
      return;
    }
    void (async () => {
      try {
        const rows = await submissions.myBest();
        const grouped = new Map<string, UserBestRow[]>();
        for (const r of rows) {
          const arr = grouped.get(r.problem_id) ?? [];
          arr.push(r);
          grouped.set(r.problem_id, arr);
        }
        bestByProblem = grouped;
      } catch (e) {
        // 401 only matters until we actually have a session; tolerate it.
        if (!(e instanceof ApiError && e.status === 401)) {
          console.warn('me/best load failed', e);
        }
      }
    })();
  });
</script>

<main class="mx-auto flex min-h-screen max-w-5xl flex-col gap-6 px-6 py-10">
  <h1 class="text-xl font-semibold tracking-tight text-zinc-100">Problems</h1>

  {#if loading}
    <p class="text-zinc-500">Loading…</p>
  {:else if error}
    <p class="text-rose-400">{error}</p>
  {:else if list.length === 0}
    <p class="text-zinc-500">
      No problems yet. {#if true}Admins can author one from the
        <a class="underline" href="/admin">admin</a> page.{/if}
    </p>
  {:else}
    <ul class="flex flex-col gap-2">
      {#each list as p (p.id)}
        {@const bests = bestByProblem.get(p.id) ?? []}
        <li>
          <a
            href={`/p/${p.id}`}
            class="flex items-baseline justify-between rounded border border-zinc-800 bg-zinc-900/40 px-4 py-3 transition hover:border-zinc-700 hover:bg-zinc-900"
          >
            <span class="flex items-baseline gap-3">
              <span class="font-mono text-xs text-zinc-500">{p.id}</span>
              <span class="text-zinc-100">{p.title}</span>
              {#if p.visibility !== 'public'}
                <span
                  class="rounded border border-zinc-700 px-1.5 py-0.5 font-mono text-[10px] uppercase text-zinc-400"
                  >{p.visibility}</span
                >
              {/if}
            </span>
            <span class="flex items-baseline gap-3 font-mono text-xs">
              {#each bests as b (b.board)}
                <span
                  class="rounded border border-emerald-800 bg-emerald-950/30 px-1.5 py-0.5 text-[10px] text-emerald-300"
                  title={`#${b.rank} · ${b.total_cycles.toLocaleString()} cy on ${b.board}`}
                >
                  {b.board} #{b.rank}
                </span>
              {/each}
              <span class="text-zinc-500">
                {p.allowed_boards.join(', ')}
              </span>
            </span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</main>
