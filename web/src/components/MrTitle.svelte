<script lang="ts">
  import { onGithub } from '../forge'
  import { emojiPieces, projectOf } from '../emoji.svelte'
  import { projectWeb } from '../mr-review.svelte'
  import type { MergeRequest } from '../types'
  import CustomEmoji from './CustomEmoji.svelte'

  /**
   * An MR's title as shown: its shortcodes as emoji, a GitLab group's custom ones as their picture;
   * without its `Draft:` unless `draft`. An attribute takes `cleanTitle`, text only.
   */
  let { mr, draft = false }: { mr: MergeRequest; draft?: boolean } = $props()

  const pieces = $derived(emojiPieces(draft ? mr.title : mr.title.replace(/^Draft:\s*/i, ''), onGithub(mr), projectOf(projectWeb(mr.url))))
</script>

{#each pieces as p, i (i)}{#if p.emoji}<CustomEmoji url={p.emoji} name={p.text} />{:else}{p.text}{/if}{/each}
