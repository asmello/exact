<script lang="ts">
  import { session } from '$lib/session.svelte';
  import { problems, ApiError, type Problem } from '$lib/api';

  const me = session();
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
  <header class="flex items-baseline justify-between">
    <h1 class="text-xl font-semibold tracking-tight text-zinc-100">Admin · problems</h1>
    <div class="flex items-center gap-3">
      <a
        href="/admin/devices"
        class="rounded border border-zinc-700 px-3 py-1 text-sm text-zinc-200 hover:bg-zinc-800"
        >devices &amp; runners</a
      >
      <a
        href="/admin/p/new"
        class="rounded border border-emerald-700 bg-emerald-900/30 px-3 py-1 text-sm text-emerald-300 hover:bg-emerald-900/50"
        >+ new problem</a
      >
    </div>
  </header>

  {#if !me.loading && !me.user?.is_admin}
    <p class="text-rose-400">You need an admin session to view this page.</p>
  {:else if loading}
    <p class="text-zinc-500">Loading…</p>
  {:else if error}
    <p class="text-rose-400">{error}</p>
  {:else if list.length === 0}
    <p class="text-zinc-500">No problems yet.</p>
  {:else}
    <ul class="flex flex-col gap-2">
      {#each list as p (p.id)}
        <li
          class="flex items-baseline justify-between rounded border border-zinc-800 bg-zinc-900/40 px-4 py-3"
        >
          <div class="flex items-baseline gap-3">
            <span class="font-mono text-xs text-zinc-500">{p.id}</span>
            <span class="text-zinc-100">{p.title}</span>
            <span class="rounded border border-zinc-700 px-1.5 py-0.5 font-mono text-[10px] uppercase text-zinc-400"
              >{p.visibility}</span
            >
          </div>
          <div class="flex items-baseline gap-3 text-sm">
            <a href={`/p/${p.id}`} class="text-zinc-400 hover:text-zinc-200">view</a>
            <a href={`/admin/p/${p.id}`} class="text-emerald-400 hover:text-emerald-300">edit</a>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</main>
