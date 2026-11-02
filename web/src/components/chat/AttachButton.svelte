<script lang="ts">
  import Icon from '../Icon.svelte'
  import { attachToDraft, filesOf } from '../../agents.svelte'
  import { t } from '../../i18n.svelte'

  /**
   * An agent's Chat box: attach images, from the Mac's files or a phone's photos and camera. Saved
   * where it reads them (`.prism/attach/`), their `@path` added to the draft, sent with it.
   */
  let { id, text, write, disabled = false, onerror, phone = false }: { id: string; text: string; write: (t: string) => void; disabled?: boolean; onerror: (e: string | null) => void; phone?: boolean } = $props()

  let input = $state<HTMLInputElement>()
  let busy = $state(false)

  async function chosen(files: File[]) {
    if (!files.length) return
    busy = true
    onerror(null)
    try {
      await attachToDraft(id, files, text, write)
    } catch (e) {
      onerror(t('Not attached: {error}', { error: String(e) }))
    } finally {
      busy = false
    }
  }
</script>

<button type="button" class="attach" class:phone disabled={disabled || busy} onclick={() => input?.click()} aria-label={t('Attach an image')} title={t('Attach an image: it is sent with your words')}><Icon name={busy ? 'refresh' : 'image'} size={phone ? 19 : 15} spin={busy} /></button>
<input bind:this={input} type="file" accept="image/*" multiple hidden onchange={(e) => { void chosen(filesOf(e.currentTarget.files)); e.currentTarget.value = '' }} />

<style>
  .attach { flex: none; display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: none; border-radius: 50%; background: none; color: var(--text-muted); cursor: pointer; }
  .attach:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  /* A phone's box: a finger's size, beside its round field. */
  .attach.phone { width: 38px; height: 38px; background: var(--surface-muted); border: 1px solid var(--border); }
  .attach:disabled { opacity: 0.4; cursor: default; }
</style>
