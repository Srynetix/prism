import { describe, expect, it } from 'vitest'
import { slug } from './agents.svelte'

describe('slug', () => {
  /** The cases of `slug_folds_every_accent` in the Rust core (`agents/mod.rs`): both sides name a worktree alike. */
  it('folds every accent, as the Rust core does', () => {
    expect(slug('ABC-4661 · Export CSV des mesures')).toBe('abc-4661-export-csv-des-mesures')
    expect(slug('  Révision: état ! ')).toBe('revision-etat')
    expect(slug('Año nuevo')).toBe('ano-nuevo')
    expect(slug('Århus')).toBe('arhus')
    expect(slug('Œuvre complète')).toBe('oeuvre-complete')
    expect(slug('Straße')).toBe('strasse')
    expect(slug('été')).toBe('ete')
    expect(slug('!!!')).toBe('')
    expect(slug('é'.repeat(100))).toBe('e'.repeat(40))
  })
})
