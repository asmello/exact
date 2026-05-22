<script lang="ts">
  import { mount } from '$lib/editor';

  // Bare function only — runtime wrapping (no_std shell, I/O glue) is the
  // build worker's job. The problem's io_spec determines the signature.
  const STARTER = `fn solve(n: u32) -> u64 {
    (1..=n as u64).sum()
}
`;

  let source = $state(STARTER);
  let editorHost: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (!editorHost) return;
    const destroy = mount(editorHost, {
      initialDoc: STARTER,
      onChange: (doc) => {
        source = doc;
      }
    });
    return destroy;
  });
</script>

<main class="mx-auto flex min-h-screen max-w-5xl flex-col gap-6 px-6 py-10">
  <header class="flex items-baseline gap-3">
    <h1 class="font-mono text-2xl font-semibold tracking-tight">exact</h1>
    <span class="text-sm text-zinc-400">cycle-accurate Cortex-M judging — step-2 skeleton</span>
  </header>

  <section class="flex flex-col gap-2">
    <label class="text-xs uppercase tracking-wider text-zinc-400" for="editor">Source</label>
    <div
      id="editor"
      bind:this={editorHost}
      class="h-96 overflow-hidden rounded-md border border-zinc-800 bg-zinc-900"
    ></div>
  </section>

  <section class="flex flex-col gap-2">
    <span class="text-xs uppercase tracking-wider text-zinc-400">Live char count</span>
    <span class="font-mono text-sm text-zinc-300">{source.length} chars</span>
  </section>
</main>
