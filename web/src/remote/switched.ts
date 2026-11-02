/**
 * A profile switch on the phone reloads the page on the other profile (`phone-profiles`): that
 * load skips the boot screen (`Boot.svelte`), which only greets a launch. Alone in its module:
 * the window's Boot reads it without the phone's code.
 */
export const NO_BOOT = 'prism.switched'

/** The page was reloaded by a profile switch: said once, the next load boots as usual. */
export function switchedHere(): boolean {
  try {
    const switched = sessionStorage.getItem(NO_BOOT) === '1'
    sessionStorage.removeItem(NO_BOOT)
    return switched
  } catch {
    return false
  }
}
