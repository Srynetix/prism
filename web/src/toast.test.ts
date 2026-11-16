import { describe, expect, it, vi } from 'vitest'
import { toast, toastUndo, toasts } from './toast.svelte'

describe('toasts', () => {
  it('keeps an Undo when ordinary toasts push past the limit', () => {
    vi.useFakeTimers()
    toastUndo('Worktree archived', () => {})
    toast('Link copied 1')
    toast('Link copied 2')
    toast('Link copied 3')
    expect(toasts.list.map((x) => x.message)).toEqual(['Worktree archived', 'Link copied 2', 'Link copied 3'])
    vi.useRealTimers()
  })
})
