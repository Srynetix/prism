<script lang="ts">
  /**
   * The file alongside: the text that saving would write, highlighted line by line, and the
   * lines that differ from the file on disk emphasised. The first modified line
   * scrolls into view.
   */
  let { text, original }: { text: string; original: string } = $props()

  interface Line {
    kind: 'comment' | 'section' | 'pair' | 'other'
    key?: string
    value?: string
    raw: string
    changed: boolean
  }
  const lines = $derived.by(() => {
    // Changed line: absent (as many times) from the original file.
    const left = new Map<string, number>()
    for (const l of original.split('\n')) left.set(l, (left.get(l) ?? 0) + 1)
    return text.split('\n').map((raw): Line => {
      const n = left.get(raw) ?? 0
      const changed = n === 0
      if (n) left.set(raw, n - 1)
      const t = raw.trim()
      if (t.startsWith('#')) return { kind: 'comment', raw, changed }
      if (t.startsWith('[')) return { kind: 'section', raw, changed }
      const m = /^(\s*[\w."-]+\s*)=(.*)$/.exec(raw)
      return m ? { kind: 'pair', key: m[1], value: m[2], raw, changed } : { kind: 'other', raw, changed }
    })
  })
  const tone = (v: string) => (/^\s*["']/.test(v) ? 's' : /^\s*[\d.-]+\s*(#.*)?$/.test(v) || /^\s*(true|false)\b/.test(v) ? 'n' : '')

  let pre: HTMLPreElement | undefined = $state()
  $effect(() => {
    void lines
    pre?.querySelector('.chg')?.scrollIntoView({ block: 'nearest' })
  })
</script>

<pre bind:this={pre}>{#each lines as l, i (i)}<span class="ln" class:chg={l.changed && l.raw.trim() !== ''}>{#if l.kind === 'comment'}<span class="c">{l.raw}</span>{:else if l.kind === 'section'}<span class="sec">{l.raw}</span>{:else if l.kind === 'pair'}<span class="k">{l.key}</span>=<span class={tone(l.value ?? '')}>{l.value}</span>{:else}{l.raw}{/if}</span>
{/each}</pre>

<style>
  pre { flex: 1; margin: 0; padding: 10px 12px; overflow: auto; color: #c9c9cf; font: 11px/1.6 var(--mono); white-space: pre; }
  .ln { display: inline-block; min-width: 100%; }
  .chg { margin: 0 -12px; padding: 0 12px; background: color-mix(in srgb, #fac775 16%, transparent); box-shadow: inset 3px 0 0 #fac775; }
  .c { color: #6b6b74; }
  .sec { color: #c678dd; }
  .k { color: #61afef; }
  .s { color: #98c379; }
  .n { color: #d19a66; }
</style>
