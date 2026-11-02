<script lang="ts">
  import { suggest } from '../../suggest'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'

  /**
   * List as chips: × removes, Enter (or a comma) adds, dragging a chip moves it
   * (order matters: statuses as columns). `options`: suggestions for the input.
   */
  let { value, onchange, options = [], placeholder = t('add…'), compact = false }: { value: string[]; onchange: (v: string[]) => void; options?: string[]; placeholder?: string; compact?: boolean } = $props()
  let draft = $state('')
  let dragged = $state<number | null>(null)

  function add() {
    const items = draft.split(',').map((x) => x.trim()).filter((x) => x && !value.includes(x))
    draft = ''
    if (items.length) onchange([...value, ...items])
  }
  function drop(to: number) {
    if (dragged === null || dragged === to) return
    const next = [...value]
    const [item] = next.splice(dragged, 1)
    next.splice(to, 0, item)
    dragged = null
    onchange(next)
  }
</script>

<div class="chips" class:compact>
  {#each value as item, i (item)}
    <span class="chip" class:dragging={dragged === i} draggable="true" role="listitem" ondragstart={() => (dragged = i)} ondragend={() => (dragged = null)} ondragover={(e) => e.preventDefault()} ondrop={() => drop(i)} title={t('Drag to reorder')}>
      {item}<button onclick={() => onchange(value.filter((_, n) => n !== i))} aria-label={t('Remove {name}', { name: item })}><Icon name="close" /></button>
    </span>
  {/each}
  <input bind:value={draft} {placeholder} {@attach suggest(() => options.filter((o) => !value.includes(o)))} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); add() } }} onblur={add} spellcheck="false" />
</div>

<style>
  .chips { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; min-height: 28px; padding: 3px 5px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); width: 100%; }
  .chips:focus-within { border-color: var(--accent); }
  .chips.compact { min-height: 26px; padding: 2px 4px; }
  .chip { display: inline-flex; align-items: center; gap: 3px; padding: 1px 3px 1px 8px; border-radius: var(--r12); background: var(--surface-muted); font: var(--fs-mono) var(--mono); cursor: grab; }
  .chip.dragging { opacity: 0.4; }
  .chip button { display: inline-flex; padding: 1px; border: none; border-radius: 50%; background: none; color: var(--text-muted); cursor: pointer; }
  .chip button:hover { color: var(--ko); }
  input { flex: 1; min-width: 80px; height: 20px; border: none; outline: none; background: none; color: var(--text); font: var(--fs-mono) var(--mono); }
</style>
