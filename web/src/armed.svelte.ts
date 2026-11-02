/**
 * Two-click confirmation, Tauri's webview having no `confirm()`: the first click arms a key (a
 * button, a row's action) for a few seconds, the second click on the same key confirms it. One
 * armer per component: arming a key disarms the other ones.
 */
export function armer(ms = 3000) {
  let key = $state<string | null>(null)
  let timer: ReturnType<typeof setTimeout> | undefined
  const disarm = () => {
    clearTimeout(timer)
    key = null
  }
  return {
    /** `k` is armed: the next click on it confirms. */
    is: (k = '') => key === k,
    /** A click on `k`: true when it confirms (`k` was armed), false when it only arms it. */
    confirm(k = '') {
      if (key === k) {
        disarm()
        return true
      }
      clearTimeout(timer)
      key = k
      timer = setTimeout(() => (key = null), ms)
      return false
    },
    disarm,
  }
}
