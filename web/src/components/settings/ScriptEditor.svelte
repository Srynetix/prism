<script lang="ts">
  import { onDestroy } from 'svelte'
  import { EditorState } from '@codemirror/state'
  import { EditorView } from '@codemirror/view'
  import { baseExtensions, compartments, sizeTheme } from '../../editor/setup'
  import { filesSettings } from '../../files-editor.svelte'
  import { languageOf } from '../../editor/languages'

  /**
   * A custom block's script, in the Files tab's editor (CodeMirror): colored by its shebang
   * (python, node, deno, bash…; sh without one), in its font and size (`[appearance] font_mono`,
   * `[files] font_size`). `value` follows what is typed; set from outside
   * (a template, an agent's draft), the editor takes it.
   */
  let { value = $bindable(''), label }: { value: string; label: string } = $props()

  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | undefined
  let lang = ''
  /** The Files tab's text size. */
  const size = $derived(filesSettings().font_size || 11.5)

  /** The file name whose language the shebang gives. */
  function pathOf(text: string): string {
    const first = text.split('\n', 1)[0]
    if (!first.startsWith('#!')) return 'script.sh'
    if (/python|uv run/.test(first)) return 'script.py'
    if (/deno|bun|tsx|ts-node/.test(first)) return 'script.ts'
    if (/node/.test(first)) return 'script.js'
    return 'script.sh'
  }

  async function setLanguage(text: string) {
    const path = pathOf(text)
    if (path === lang || !view) return
    lang = path
    const ext = await languageOf(path)
    view?.dispatch({ effects: compartments.language.reconfigure(ext ?? []) })
  }

  $effect(() => {
    if (!host || view) return
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: baseExtensions({ vim: false, wrap: false, fontSize: size, readOnly: false }, [
          EditorView.updateListener.of((u) => {
            if (!u.docChanged) return
            value = u.state.doc.toString()
            void setLanguage(value)
          }),
          EditorView.contentAttributes.of({ 'aria-label': label }),
        ]),
      }),
    })
    void setLanguage(value)
  })

  // The Files tab's size changed: the editor follows.
  $effect(() => {
    const px = size
    view?.dispatch({ effects: compartments.size.reconfigure(sizeTheme(px)) })
  })

  // Set from outside: the editor follows.
  $effect(() => {
    const v = value
    if (view && v !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } })
      void setLanguage(v)
    }
  })

  onDestroy(() => view?.destroy())
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor { height: 300px; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; background: var(--surface); }
  .editor :global(.cm-editor) { height: 100%; }
</style>
