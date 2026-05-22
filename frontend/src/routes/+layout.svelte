<script lang="ts">
  import '../app.css';
  import { session, logout } from '$lib/session.svelte';

  let { children } = $props();
  const me = session();
</script>

<header
  class="sticky top-0 z-10 flex items-center justify-between border-b border-zinc-900 bg-zinc-950/80 px-6 py-3 backdrop-blur"
>
  <a href="/" class="flex items-baseline gap-3 no-underline">
    <span class="font-mono text-lg font-semibold tracking-tight text-zinc-50">exact</span>
    <span class="text-xs text-zinc-500">cycle-accurate Cortex-M judging</span>
  </a>

  <nav class="flex items-center gap-3 text-sm">
    {#if me.loading}
      <span class="text-zinc-500">…</span>
    {:else if me.user}
      {#if me.user.avatar_url}
        <img
          src={me.user.avatar_url}
          alt={me.user.github_login}
          class="h-6 w-6 rounded-full border border-zinc-800"
        />
      {/if}
      <span class="text-zinc-300">{me.user.github_login}</span>
      {#if me.user.is_admin}
        <span
          class="rounded border border-emerald-700 px-1.5 py-0.5 font-mono text-[10px] uppercase text-emerald-400"
          >admin</span
        >
      {/if}
      <button
        type="button"
        onclick={() => logout()}
        class="rounded border border-zinc-800 px-2 py-1 text-xs text-zinc-300 hover:border-zinc-700 hover:text-zinc-100"
      >
        log out
      </button>
    {:else}
      <a
        href="/auth/github"
        class="rounded border border-zinc-800 px-3 py-1 text-zinc-200 hover:border-zinc-700 hover:bg-zinc-900"
      >
        sign in with github
      </a>
    {/if}
  </nav>
</header>

{@render children()}
