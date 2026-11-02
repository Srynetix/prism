import { isDesktop } from './bridge'
import { homeDir } from '@tauri-apps/api/path'
import { open } from '@tauri-apps/plugin-dialog'

/** The system's picker exists only in the app: the browser preview types paths. */
export const canPick = () => isDesktop()

let home: Promise<string> | null = null
const homeOf = () => (home ??= homeDir().then((h) => h.replace(/\/+$/, '')))

/**
 * A folder, or a file with `extensions`, chosen in the system's picker, opened where `current`
 * points; under the home folder, written `~/…` as config.toml does. Null: cancelled.
 */
export async function pickPath(current: string, { file = false, extensions }: { file?: boolean; extensions?: string[] } = {}): Promise<string | null> {
  const h = await homeOf()
  const now = current.trim()
  const start = now.startsWith('~') ? h + now.slice(1) : now
  const picked = await open({
    directory: !file,
    multiple: false,
    defaultPath: start.startsWith('/') ? start : h,
    filters: file && extensions ? [{ name: extensions.join(', '), extensions }] : undefined,
  })
  if (typeof picked !== 'string') return null
  return picked === h || picked.startsWith(`${h}/`) ? `~${picked.slice(h.length)}` : picked
}
