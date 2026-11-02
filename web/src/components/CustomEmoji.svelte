<script lang="ts">
  import { invoke } from '../bridge'

  /**
   * A GitLab group's custom emoji in a text: its picture, read by the Rust core (the window's CSP
   * allows no external image) and kept for the session, at the text's height; its `:name:` while it
   * loads, or if it cannot be read (a picture linked from outside the instance).
   */
  let { url, name }: { url: string; name: string } = $props()

  let src = $state<string | null>(null)
  $effect(() => {
    const u = url
    src = null
    load(u).then((s) => u === url && (src = s), () => {})
  })
</script>

<script module lang="ts">
  const cache = new Map<string, Promise<string>>()
  function load(url: string): Promise<string> {
    let p = cache.get(url)
    if (!p) {
      p = invoke<string>('forge_emoji_image', { url })
      cache.set(url, p)
    }
    return p
  }
</script>

{#if src}<img class="emoji" {src} alt={name} title={name} />{:else}<span>{name}</span>{/if}

<style>
  .emoji { display: inline-block; height: 1.25em; width: auto; max-width: 3em; vertical-align: -0.25em; object-fit: contain; }
</style>
