import { call } from '../block'

/**
 * Refresh of the Data and BigQuery cost blocks: only one in progress, shared by both buttons. The
 * core re-reads GCP (cost included) and only hands back once the read is in the snapshot.
 */
export const dataRefresh = $state({ busy: false, error: '' })

export async function refreshData() {
  if (dataRefresh.busy) return
  dataRefresh.busy = true
  dataRefresh.error = ''
  try {
    await call('data', 'refresh')
  } catch (e) {
    dataRefresh.error = String(e)
  } finally {
    dataRefresh.busy = false
  }
}
