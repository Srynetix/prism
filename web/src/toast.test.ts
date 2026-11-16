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

  it('keeps a repeated message in place, and two Undo apart', () => {
    vi.useFakeTimers()
    toasts.list = []
    toast('Saved')
    const id = toasts.list[0].id
    toast('Saved')
    expect(toasts.list.map((x) => x.id)).toEqual([id])
    toasts.list = []
    toastUndo('Page deleted', () => {})
    toastUndo('Page deleted', () => {})
    expect(toasts.list.length).toBe(2)
    vi.useRealTimers()
  })
})
