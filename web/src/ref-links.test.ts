import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Snapshot } from './types'

// The tracker's module follows the page's address (filter.svelte.ts): none under Node.
const page = globalThis as { location?: unknown; addEventListener?: unknown }
page.location ??= { hash: '' }
page.addEventListener ??= () => {}

const opened = vi.hoisted(() => ({ browser: [] as string[] }))
vi.mock('./links', () => ({ openLink: (url: string) => opened.browser.push(url) }))
vi.mock('./bridge', async (real) => ({ ...(await real<object>()), hasCore: () => true }))
vi.mock('./ticket-view.svelte', () => ({ canPreview: () => true, openTicket: vi.fn() }))

const { openUrlOrRef } = await import('./ref-links')
const { live } = await import('./live.svelte')
const { mrView } = await import('./mr-view.svelte')
const { openTicket } = await import('./ticket-view.svelte')

live.snapshot = {
  conventions: { ticket_prefix: 'SHOP' },
  ticket_base: 'https://tracker.example.org/browse/',
  merge_requests: [{ iid: 1288, repo: null, url: 'https://gitlab.example.org/shop/app/-/merge_requests/1288' }],
  sprint: { tickets: [] },
  pickup: null,
  time_tasks: null,
  next_sprint: null,
  outside_sprint: { tickets: [] },
} as unknown as Snapshot

describe('openUrlOrRef', () => {
  beforeEach(() => {
    opened.browser = []
    mrView.ref = null
  })
  it('opens a local server or any other page in the browser', () => {
    openUrlOrRef('http://portfolio.localhost:3000/')
    openUrlOrRef('https://docs.example.org/guide')
    expect(opened.browser).toEqual(['http://portfolio.localhost:3000/', 'https://docs.example.org/guide'])
    expect(mrView.ref).toBeNull()
  })
  it('opens an MR or a ticket in Prism, in the browser when asked', () => {
    openUrlOrRef('https://gitlab.example.org/shop/app/-/merge_requests/1301/diffs')
    expect(mrView.ref).toEqual({ iid: 1301, repo: null })
    openUrlOrRef('https://tracker.example.org/browse/SHOP-401')
    expect(openTicket).toHaveBeenCalledWith('SHOP-401')
    expect(opened.browser).toEqual([])
    openUrlOrRef('https://gitlab.example.org/shop/app/-/merge_requests/1301', true)
    expect(opened.browser).toEqual(['https://gitlab.example.org/shop/app/-/merge_requests/1301'])
  })
})
