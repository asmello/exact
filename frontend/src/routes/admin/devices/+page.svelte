<script lang="ts">
  import { session } from '$lib/session.svelte';
  import {
    devices,
    runners,
    ApiError,
    type Device,
    type Runner,
    type Board,
    type CreateRunnerResponse
  } from '$lib/api';

  const me = session();
  let deviceList = $state<Device[]>([]);
  let runnerList = $state<Runner[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // New-device form
  let newDeviceId = $state('qemu-local');
  let newDeviceBoard = $state<Board>('lm3s6965evb');
  let newDeviceCclkHz = $state(12_000_000);
  let newDeviceDesc = $state('local qemu');
  let newDeviceSynthetic = $state(true);
  let deviceError = $state<string | null>(null);

  // New-runner form
  let newRunnerDeviceId = $state('qemu-local');
  let newRunnerLabel = $state('qemu (local dev)');
  let lastCreatedRunner = $state<CreateRunnerResponse | null>(null);
  let runnerError = $state<string | null>(null);

  async function reload() {
    loading = true;
    error = null;
    try {
      [deviceList, runnerList] = await Promise.all([devices.list(), runners.list()]);
    } catch (e) {
      error = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void reload();
  });

  async function addDevice() {
    deviceError = null;
    try {
      await devices.create({
        id: newDeviceId,
        board: newDeviceBoard,
        cclk_hz: newDeviceCclkHz,
        description: newDeviceDesc || undefined,
        synthetic: newDeviceSynthetic
      });
      await reload();
    } catch (e) {
      deviceError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }

  async function addRunner() {
    runnerError = null;
    try {
      lastCreatedRunner = await runners.create(newRunnerDeviceId, newRunnerLabel);
      await reload();
    } catch (e) {
      runnerError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }

  async function revokeRunner(id: number) {
    if (!confirm(`Revoke runner #${id}?`)) return;
    try {
      await runners.revoke(id);
      await reload();
    } catch (e) {
      runnerError = e instanceof ApiError ? `${e.status}: ${e.message}` : String(e);
    }
  }
</script>

<main class="mx-auto flex min-h-screen max-w-5xl flex-col gap-8 px-6 py-10">
  <header class="flex items-baseline justify-between">
    <h1 class="text-xl font-semibold tracking-tight text-zinc-100">Admin · devices &amp; runners</h1>
    <a href="/admin" class="text-sm text-zinc-400 hover:text-zinc-200">← back</a>
  </header>

  {#if lastCreatedRunner}
    <div
      class="sticky top-16 z-20 flex flex-col gap-2 rounded border border-amber-700 bg-amber-950/80 px-4 py-3 text-amber-100 backdrop-blur"
    >
      <div class="flex items-baseline justify-between">
        <span class="text-xs uppercase tracking-wider text-amber-400">
          one-shot token for runner #{lastCreatedRunner.runner.id} ({lastCreatedRunner.runner.label}) — shown once
        </span>
        <button
          type="button"
          onclick={() => (lastCreatedRunner = null)}
          class="text-xs text-amber-400 hover:text-amber-200">dismiss</button
        >
      </div>
      <div class="flex items-center gap-2">
        <input
          type="text"
          readonly
          value={lastCreatedRunner.token}
          onclick={(e) => (e.currentTarget as HTMLInputElement).select()}
          class="flex-1 rounded border border-amber-900 bg-amber-950/60 px-2 py-1 font-mono text-xs text-amber-50"
        />
        <button
          type="button"
          onclick={async () => {
            if (lastCreatedRunner) {
              await navigator.clipboard.writeText(lastCreatedRunner.token);
            }
          }}
          class="rounded border border-amber-700 px-3 py-1 text-xs text-amber-300 hover:bg-amber-900/40"
          >copy</button
        >
      </div>
      <p class="text-xs text-amber-300/80">
        Save to a file (e.g. <code>runner.token</code>) and pass via
        <code>--api-key-file</code>. This won't be shown again.
      </p>
    </div>
  {/if}

  {#if !me.loading && !me.user?.is_admin}
    <p class="text-rose-400">You need an admin session to view this page.</p>
  {:else if loading}
    <p class="text-zinc-500">Loading…</p>
  {:else if error}
    <p class="text-rose-400">{error}</p>
  {:else}
    <!-- Devices -->
    <section class="flex flex-col gap-4">
      <h2 class="text-lg font-semibold text-zinc-100">Devices</h2>
      {#if deviceList.length === 0}
        <p class="text-sm text-zinc-500">No devices yet.</p>
      {:else}
        <ul class="flex flex-col gap-2">
          {#each deviceList as d (d.id)}
            <li
              class="flex items-baseline justify-between rounded border border-zinc-800 bg-zinc-900/40 px-4 py-3 font-mono text-sm"
            >
              <div class="flex items-baseline gap-3">
                <span class="text-zinc-100">{d.id}</span>
                <span class="text-zinc-500">{d.board}</span>
                <span class="text-zinc-500">{(d.cclk_hz / 1_000_000).toFixed(1)} MHz</span>
                {#if d.synthetic}
                  <span class="rounded border border-amber-800 bg-amber-950/40 px-1.5 py-0.5 text-[10px] uppercase text-amber-400"
                    >synthetic</span
                  >
                {/if}
              </div>
              <div class="flex items-baseline gap-3 text-xs">
                {#if d.online}
                  <span class="text-emerald-400">● online</span>
                {:else}
                  <span class="text-zinc-500">○ offline</span>
                {/if}
                {#if d.last_seen}
                  <span class="text-zinc-500">last {d.last_seen}</span>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}

      <details class="rounded border border-zinc-800 bg-zinc-900/30 px-4 py-3 text-sm">
        <summary class="cursor-pointer text-zinc-300">+ register a device</summary>
        <div class="mt-4 grid grid-cols-2 gap-3">
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">id</span>
            <input
              type="text"
              bind:value={newDeviceId}
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">board</span>
            <select
              bind:value={newDeviceBoard}
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
            >
              <option value="lm3s6965evb">lm3s6965evb (qemu)</option>
              <option value="lpc1768">lpc1768</option>
              <option value="stm32f429zi">stm32f429zi</option>
            </select>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">cclk_hz</span>
            <input
              type="number"
              bind:value={newDeviceCclkHz}
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
            />
          </label>
          <label class="flex items-center gap-2 text-sm text-zinc-200">
            <input type="checkbox" bind:checked={newDeviceSynthetic} />
            synthetic cycles
          </label>
          <label class="col-span-2 flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">description</span>
            <input
              type="text"
              bind:value={newDeviceDesc}
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
            />
          </label>
        </div>
        <div class="mt-3 flex items-center gap-3">
          <button
            type="button"
            onclick={addDevice}
            class="rounded border border-emerald-700 bg-emerald-900/30 px-3 py-1 text-sm text-emerald-300 hover:bg-emerald-900/50"
            >register</button
          >
          {#if deviceError}<span class="text-sm text-rose-400">{deviceError}</span>{/if}
        </div>
      </details>
    </section>

    <!-- Runners -->
    <section class="flex flex-col gap-4">
      <h2 class="text-lg font-semibold text-zinc-100">Runners</h2>
      {#if runnerList.length === 0}
        <p class="text-sm text-zinc-500">No runners yet.</p>
      {:else}
        <ul class="flex flex-col gap-2">
          {#each runnerList as r (r.id)}
            <li
              class="flex items-baseline justify-between rounded border border-zinc-800 bg-zinc-900/40 px-4 py-3 font-mono text-sm"
            >
              <div class="flex items-baseline gap-3">
                <span class="text-zinc-500">#{r.id}</span>
                <span class="text-zinc-100">{r.label}</span>
                <span class="text-zinc-500">→ {r.device_id}</span>
                <span class="text-zinc-600">{r.token_prefix}…</span>
              </div>
              <div class="flex items-baseline gap-3 text-xs">
                {#if r.revoked_at}
                  <span class="text-rose-400">revoked</span>
                {:else}
                  <button
                    type="button"
                    onclick={() => revokeRunner(r.id)}
                    class="text-rose-400 hover:text-rose-300">revoke</button
                  >
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}

      <details class="rounded border border-zinc-800 bg-zinc-900/30 px-4 py-3 text-sm">
        <summary class="cursor-pointer text-zinc-300">+ provision a runner</summary>
        <div class="mt-4 grid grid-cols-2 gap-3">
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">device</span>
            <select
              bind:value={newRunnerDeviceId}
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100"
            >
              {#each deviceList as d (d.id)}
                <option value={d.id}>{d.id}</option>
              {/each}
            </select>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-zinc-400">label</span>
            <input
              type="text"
              bind:value={newRunnerLabel}
              class="rounded border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
            />
          </label>
        </div>
        <div class="mt-3 flex items-center gap-3">
          <button
            type="button"
            onclick={addRunner}
            class="rounded border border-emerald-700 bg-emerald-900/30 px-3 py-1 text-sm text-emerald-300 hover:bg-emerald-900/50"
            >provision</button
          >
          {#if runnerError}<span class="text-sm text-rose-400">{runnerError}</span>{/if}
        </div>

      </details>
    </section>
  {/if}
</main>
