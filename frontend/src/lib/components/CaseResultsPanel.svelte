<script lang="ts">
  import type { Submission } from '$lib/api';
  import { decodeOutputB64, type IoSpec } from '$lib/bytes';

  interface Props {
    submission: Submission;
    outputSpec?: IoSpec['output'];
  }

  const { submission, outputSpec }: Props = $props();
</script>

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
            <span
              class="rounded border border-rose-800 bg-rose-950/40 px-1.5 py-0.5 text-[10px] uppercase text-rose-400"
              >WRONG</span
            >
          {:else if r.passed === true}
            <span
              class="rounded border border-emerald-800 bg-emerald-950/40 px-1.5 py-0.5 text-[10px] uppercase text-emerald-400"
              >PASS</span
            >
          {:else}
            <span
              class="rounded border border-zinc-700 px-1.5 py-0.5 text-[10px] uppercase text-zinc-400"
              >OK</span
            >
          {/if}
        {:else}
          <span
            class="rounded border border-rose-800 bg-rose-950/40 px-1.5 py-0.5 text-[10px] uppercase text-rose-400"
            >{r.status}</span
          >
        {/if}
        <span class="truncate text-zinc-400">
          {#if r.output !== null}
            out: {decodeOutputB64(r.output, outputSpec) || '(empty)'}
          {/if}
        </span>
        <span class="text-zinc-300">
          {r.cycles !== null ? `${r.cycles.toLocaleString()} cy` : '—'}
        </span>
        {#if r.synthetic}
          <span
            class="rounded border border-amber-800 bg-amber-950/30 px-1.5 py-0.5 text-[10px] uppercase text-amber-400"
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
