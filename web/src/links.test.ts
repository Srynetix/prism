import { describe, expect, it, vi } from 'vitest'

vi.mock('./bridge', () => ({ isDesktop: () => false }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))

describe('openLink', () => {
  it('opens the web, Claude and Obsidian, never another scheme a link brings', async () => {
    const open = vi.fn()
    vi.stubGlobal('window', { open })
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const { openLink } = await import('./links')
    for (const href of ['https://gitlab.example.org/a/-/merge_requests/1', 'http://localhost:5173', 'claude://code/continue?session=local_1', 'obsidian://open?vault=v']) openLink(href)
    for (const href of ['vscode://vscode-remote/ssh-remote+x/etc', 'file:///etc/passwd', 'javascript:alert(1)', ' smb://host/share']) openLink(href)
    expect(open.mock.calls.map((c) => c[0])).toEqual(['https://gitlab.example.org/a/-/merge_requests/1', 'http://localhost:5173', 'claude://code/continue?session=local_1', 'obsidian://open?vault=v'])
    vi.unstubAllGlobals()
  })
})
