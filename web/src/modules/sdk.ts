/**
 * `@prism/sdk`: what a module's components (`components.js`) reach of Prism, at run time, as
 * Prism's own instances (one Svelte, one snapshot, one set of preferences). Adding a name is
 * adding to the module contract; removing or changing one breaks the modules built against it.
 */
// Words.
export { t, tc, tn } from '../i18n.svelte'
// Data: the snapshot, a block's state, its commands.
export { live } from '../live.svelte'
export { call, stateOf } from '../blocks/block'
export { hasCore } from '../bridge'
export { featureOn } from '../fun.svelte'
// Layout: the block's size, preferences, where it is placed.
export { columnsOf, isCollapsed, isShort, pref, setPref, showBlock } from '../layout.svelte'
export { views } from '../views'
// Prism's actions: a link, an MR, a ticket, a job's logs, a toast.
export { linkOnly, openLink } from '../links'
export { openMr } from '../mr-view.svelte'
export { canPreview, previewClick } from '../ticket-view.svelte'
export { canShowLogs, logsClick, openJob, openPipeline } from '../logs.svelte'
export { toast } from '../toast.svelte'
// Popups and keys.
export { anchored, exclusive } from '../popup'
export { keys } from '../platform'
// Formats.
export { locale } from '../locale'
export { ago, minutesSince } from '../time'
// The forge and the team's conventions; ArgoCD.
export { conventions, isRelease, ticketPrefix } from '../conventions'
export { mrNumber } from '../forge'
export { argoUrl, findApp, rollout } from '../argocd'
// Components.
export { default as Icon } from '../components/Icon.svelte'
export { default as Logo } from '../components/Logo.svelte'
export { default as BlockControls } from '../components/BlockControls.svelte'
export { default as StageManualJobs } from '../components/StageManualJobs.svelte'
