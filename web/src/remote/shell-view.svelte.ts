/** An agent's shell open on the phone, over its terminal (Shell or Tasks): its terminal key and title. */
export const shellView = $state({ key: null as string | null, title: '' })

export function openShellView(key: string, title: string) {
  shellView.key = key
  shellView.title = title
}

export const closeShellView = () => (shellView.key = null)
