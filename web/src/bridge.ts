/**
 * The page's way to Prism's core: everything the window asks of it, and hears from it, goes
 * through here rather than through Tauri directly. In the desktop window it is Tauri's IPC; on a
 * phone, the remote's page plugs its socket in (`useTransport`) before anything calls the core.
 *
 * Two questions, apart:
 * - `hasCore()`: a core answers this page (not the browser preview, which reads a file);
 * - `isDesktop()`: this page is the Mac's own window: its menu, Dock, notifications, dialogs,
 *   settings, and what opens an app on the Mac. A phone has a core, never a desktop.
 */
import { Channel, invoke as tauriInvoke, isTauri, type InvokeArgs } from '@tauri-apps/api/core'
import { listen as tauriListen, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event'

export { Channel }

/** Another way to the core than Tauri's: the remote's socket. */
export interface Transport {
  invoke<T>(command: string, args?: InvokeArgs): Promise<T>
  listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn>
}

let transport: Transport | null = null

/** The remote's page: its socket answers from now on. */
export function useTransport(t: Transport) {
  transport = t
}

/** A core answers this page: the desktop window, or a phone's page through its socket. */
export const hasCore = (): boolean => isTauri() || transport !== null

/** This page is the desktop window. */
export const isDesktop = (): boolean => isTauri()

/** Told of each command the core refused or failed (`problems.svelte.ts`), the caller still told too. */
let failed: ((command: string, error: unknown) => void) | null = null

export function onCommandFailed(f: (command: string, error: unknown) => void) {
  failed = f
}

/** A command of the core (`src-tauri/src/commands/`), its answer. */
export function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  const answer = transport ? transport.invoke<T>(command, args) : tauriInvoke<T>(command, args)
  return failed ? answer.catch((e: unknown) => {
    failed?.(command, e)
    throw e
  }) : answer
}

/** An event the core pushes (`snapshot`, `menu`…); the returned function stops listening. */
export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return transport ? transport.listen<T>(event, handler) : tauriListen<T>(event, handler)
}
