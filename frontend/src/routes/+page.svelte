<script lang="ts">
  import { problems, ApiError, type Problem } from '$lib/api';

  let list = $state<Problem[]>([]);
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
        <li>
          <a
            href={`/p/${p.id}`}
            class="flex items-baseline justify-between rounded border border-zinc-800 bg-zinc-900/40 px-4 py-3 transition hover:border-zinc-700 hover:bg-zinc-900"
          >
            <span class="flex items-baseline gap-3">
              <span class="font-mono text-xs text-zinc-500">{p.id}</span>
              <span class="text-zinc-100">{p.title}</span>
              {#if p.visibility !== 'public'}
                <span class="rounded border border-zinc-700 px-1.5 py-0.5 font-mono text-[10px] uppercase text-zinc-400"
                  >{p.visibility}</span
                >
              {/if}
            </span>
            <span class="font-mono text-xs text-zinc-500">
              {p.allowed_boards.join(', ')}
            </span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</main>
