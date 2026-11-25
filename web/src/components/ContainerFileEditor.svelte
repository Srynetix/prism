<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import { EditorState } from '@codemirror/state'
  import { EditorView, keymap } from '@codemirror/view'
  import { baseExtensions, compartments, sizeTheme } from '../editor/setup'
  import { filesFontSize } from '../files-editor.svelte'
  import { languageOf } from '../editor/languages'

  /**
   * A container's text file edited (`ContainerFiles.svelte`), in the Files tab's editor
   * (CodeMirror): its language by its name, its line endings kept (a file in CRLF stays so).
   * `against`: what differs from that text is shown (a production save's review, read-only; a
   * conflict, each change kept or put back). ⌘S saves.
   */
  let { path, value = $bindable(''), against = null, readOnly = false, onsave }: { path: string; value: string; against?: string | null; readOnly?: boolean; onsave: () => void } = $props()

  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | undefined
  let made = 0
  const size = $derived(filesFontSize())

  $effect(() => {
    if (!host || view) return
    const doc = untrack(() => value)
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc,
        extensions: baseExtensions({ vim: false, wrap: false, fontSize: (made = untrack(() => size)), readOnly: untrack(() => readOnly) }, [
          // CodeMirror reads any line ending and writes `\n`: a CRLF file keeps its own.
          doc.includes('\r\n') ? EditorState.lineSeparator.of('\r\n') : [],
          keymap.of([{ key: 'Mod-s', preventDefault: true, run: () => (onsave(), true) }]),
          EditorView.updateListener.of((u) => {
            // `sliceDoc`, not `doc.toString()`: the latter joins lines with `\n` whatever the separator.
            if (u.docChanged) value = u.state.sliceDoc()
          }),
          EditorView.contentAttributes.of({ 'aria-label': path }),
        ]),
      }),
    })
    void languageOf(untrack(() => path)).then((ext) => view?.dispatch({ effects: compartments.language.reconfigure(ext ?? []) }))
    view.focus()
  })

  $effect(() => {
    const px = size
    if (!view || px === made) return
    made = px
    view.dispatch({ effects: compartments.size.reconfigure(sizeTheme(px)) })
  })

  $effect(() => {
    const ro = readOnly
    view?.dispatch({ effects: compartments.readOnly.reconfigure(EditorState.readOnly.of(ro)) })
  })

  // The changes against `against`, shown inline: kept or put back one by one when editable.
  $effect(() => {
    const original = against
    const controls = !readOnly
    if (!view) return
    if (original === null) {
      view.dispatch({ effects: compartments.merge.reconfigure([]) })
      return
    }
    void import('@codemirror/merge').then(({ unifiedMergeView }) => {
      view?.dispatch({ effects: compartments.merge.reconfigure(unifiedMergeView({ original, mergeControls: controls, gutter: false, syntaxHighlightDeletions: true })) })
    })
  })

  // Set from outside (the container's text taken after a conflict): the editor follows.
  $effect(() => {
    const v = value
    if (view && v !== view.state.sliceDoc()) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } })
  })

  onDestroy(() => view?.destroy())
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor { flex: 1; min-height: 0; overflow: hidden; background: var(--surface); }
  .editor :global(.cm-editor) { height: 100%; }
</style>
