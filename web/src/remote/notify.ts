import { invoke } from '../bridge'
import { t } from '../i18n.svelte'

/**
 * This phone's notifications (`[remote] push`): subscribed with the Mac's VAPID key, the
 * subscription's endpoint handed to the Mac, which pushes nothing but a wake-up.
 */
const key = (b64: string): Uint8Array<ArrayBuffer> => Uint8Array.from(atob(b64.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0))

export const pushSupported = () => 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window

/** Whether this phone is subscribed now. */
export async function subscribed(): Promise<boolean> {
  if (!pushSupported()) return false
  const reg = await navigator.serviceWorker.ready
  return !!(await reg.pushManager.getSubscription()) && Notification.permission === 'granted'
}

/** Turns this phone's notifications on (asks the permission) or off. */
export async function setNotifications(on: boolean, vapid: string) {
  const reg = await navigator.serviceWorker.ready
  const current = await reg.pushManager.getSubscription()
  if (!on) {
    await current?.unsubscribe()
    await invoke('remote_push', { endpoint: null })
    return
  }
  if ((await Notification.requestPermission()) !== 'granted') throw new Error(t('notifications are refused for this page: allow them in the browser’s site settings'))
  const sub = current ?? (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key(vapid) }))
  await invoke('remote_push', { endpoint: sub.endpoint })
}
