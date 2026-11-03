import { invoke } from './bridge'
import { t } from './i18n.svelte'
import { openLink } from './links'
import { toast } from './toast.svelte'

/**
 * A dev server opened: at the desk, its local address; on a phone, where `localhost` means
 * nothing, the remote's preview (`[remote] preview`), which forwards to the dev server chosen
 * last. `phone` and `on` come with the phone's hello.
 */
export const devPreview = $state({ phone: false, on: false })

export function openDevServer(port: number) {
  if (!devPreview.phone) return openLink(`http://localhost:${port}`)
  if (!devPreview.on) return toast(t('The preview is off: Settings › Remote › Exposure on the computer'), 5000)
  // Opened now, from the tap (a page opened after an await is blocked), then sent to its address;
  // the preview never reaches back to this page.
  const tab = window.open('', '_blank')
  if (tab) tab.opener = null
  invoke<{ url: string }>('preview_open', { port }).then(
    ({ url }) => (tab ? (tab.location.href = url) : window.open(url, '_blank', 'noopener')),
    (e) => {
      tab?.close()
      toast(String(e), 5000)
    },
  )
}
