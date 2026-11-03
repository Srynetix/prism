import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'

/**
 * The system's notifications, on the computer: asked for once, then sent. Every module that notifies
 * (desk events, digest, agenda, reminders) goes through here; the chains notify from Rust.
 */
let permission: Promise<boolean> | null = null

export async function canNotify(): Promise<boolean> {
  permission ??= (async () => (await isPermissionGranted()) || (await requestPermission()) === 'granted')()
  return permission
}

/** A notification, once `canNotify` said yes. */
export const notify = (n: { title: string; body: string }) => sendNotification(n)
