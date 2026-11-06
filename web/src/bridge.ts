/**
 * The page's way to Prism's core: everything the window asks of it, and hears from it, goes
 * through here rather than through Tauri directly. In the desktop window it is Tauri's IPC; on a
 * phone, the remote's page plugs its socket in (`useTransport`) before anything calls the core.
 *
 * Three questions, apart:
 * - `hasCore()`: a core answers this page (not the browser preview, which reads a file);
 * - `isDesktop()`: this page is a desktop window: its menu, notifications, dialogs, settings. A
 *   phone has a core, never a desktop;
 * - `isLocal()`: that core is this computer's, not another machine's (`machine()`): its paths and
 *   apps are the ones here. A window of another profile of this computer (`profileWindow()`) is;
 * - `isMain()`: the app's main window, whose own profile it shows: the menu bar's count is its.
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

/**
 * The machine this window shows (`[[machines]]`): another computer's Prism, which its calls go to
 * (`machine.svelte.ts`); `null`, this computer's. Told by the app before any script (`machines.rs`).
 */
export const machine = (): string | null => (globalThis as { __PRISM_MACHINE__?: string }).__PRISM_MACHINE__ ?? null

/** The machine's profile this window shows, when not the one its remote answers at (the hub's). */
export const machineProfile = (): string | null => (globalThis as { __PRISM_MACHINE_PROFILE__?: string | null }).__PRISM_MACHINE_PROFILE__ ?? null

/**
 * Another profile of this computer this window shows (`profile_windows.rs`): its Prism runs
 * without a window, reached through the app (`machine.svelte.ts`, locally); `null`, the app's own.
 */
export const profileWindow = (): string | null => (globalThis as { __PRISM_PROFILE_WINDOW__?: string }).__PRISM_PROFILE_WINDOW__ ?? null

/**
 * The core runs on this computer: the paths it answers are this computer's, its apps and its file
 * manager too (pickers, Reveal in Finder, Open in…). A desktop window, not a machine's.
 */
export const isLocal = (): boolean => isDesktop() && machine() === null

/** The app's main window: its own profile's, neither a machine's nor another profile's. */
export const isMain = (): boolean => isLocal() && profileWindow() === null

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

/**
 * A command of this computer's app, whatever core the page talks to: a machine's window
 * (`machine.svelte.ts`) asks its own app for what is the window's, and for the machine's calls.
 */
export function localInvoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  return tauriInvoke<T>(command, args)
}

/** An event of this computer's app, whatever core the page talks to (as `localInvoke`). */
export function localListen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return tauriListen<T>(event, handler)
}

/** An event the core pushes (`snapshot`, `menu`…); the returned function stops listening. */
export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return transport ? transport.listen<T>(event, handler) : tauriListen<T>(event, handler)
}
