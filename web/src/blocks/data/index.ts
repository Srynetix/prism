import type { BlockAlert, BlockDef } from '../block'
import { dataProblems } from './data'
import type { GcpData } from './types'
import guide from './13-data.md?raw'
import guideFr from './13-data.fr.md?raw'
import { t, tn } from '../../i18n.svelte'

/**
 * Data: what is broken (job failed or not started, late table whose job failed, stream
 * failed) notifies; what drifts (slow job, late table, lagging stream, cost spike) does not.
 */
function alerts(d: GcpData): BlockAlert[] {
  return dataProblems(d).map((p) => ({
    severity: p.state.tone === 'ko' ? 'ko' : 'warn',
    text: `${t('{name}: {state}', { name: p.name, state: p.state.label })}${p.state.why ? ` — ${p.state.why}` : ''}`,
    url: p.url,
    notify: p.state.tone === 'ko',
    // One notification per transition to the broken state: the object, not its changing detail.
    notification: { key: `data:${p.name}`, title: t('Data pipeline') },
  }))
}

/** Broken jobs, tables or streams (not drifting ones): the badge of the pages showing them. */
function badge(d: GcpData) {
  const count = dataProblems(d).filter((p) => p.state.tone === 'ko').length
  return { count, title: tn(count, '{n} data problem', '{n} data problems') }
}

/** The Data block: the pipelines on GCP. Its Rust side (`data`) also feeds BigQuery cost. */
export const data = {
  id: 'data' as const,
  label: t('Data'),
  category: 'Data',
  help: {
    purpose: t('Is a data pipeline on GCP broken, late or drifting?'),
    points: [
      t('Cloud Run jobs: last executions, failures in a row and their exit code, a run Cloud Scheduler could not start, a run far longer than its median ([data] slow_factor).'),
      t('BigQuery tables declared in [[data.tables]]: late when older than max_age_hours; red when the job that feeds them (fed_by) failed.'),
      t('Scheduled queries that failed, Datastream streams failed, paused, or lagging ([data] stream_lag_minutes).'),
      t('Problems by default, All for the rest; a click unfolds a job or a stream, with links to the console and the logs. ↻ reads GCP again now.'),
    ],
    source: t('GCP APIs with the gcloud login, read-only, every [data] poll_seconds. Metadata only: never rows, schemas, SQL or log lines.'),
  },
  guide: { file: '13-data.md', text: guide, translations: { fr: guideFr } },
  boot: { label: t('Polling GCP pipelines'), detail: (d) => `${tn(d.jobs.length, '{n} job', '{n} jobs')} · ${tn(d.streams.length, '{n} stream', '{n} streams')}` },
  badge,
  alerts,
  settings: {
    parent: 'infra',
    node: { id: 'data', label: t('Data (GCP)'), sections: ['data'], toggle: 'data' },
    sections: [
      {
        id: 'data', source: 'data', label: t('Data (GCP)'), table: '[data]', path: ['data'],
        intro: t('Data pipelines on GCP, read-only with the gcloud login (gcloud auth login): Cloud Run jobs, BigQuery freshness and cost, scheduled queries, Datastream. Metadata only: never rows, schemas, SQL, log lines, nor who ran a query.'),
        fields: [
          { key: 'enabled', label: t('GCP access'), hint: t('Off by default. On: the Data and BigQuery cost blocks are read (add them to a page, or use a Data template from a pack in the pages wizard).'), kind: 'toggle', check: 'gcloud' },
          { key: 'projects', label: t('Projects'), hint: t('GCP project ids read.'), kind: 'list' },
          { key: 'region', label: t('Region'), hint: t('Of the Cloud Run jobs, their Cloud Scheduler triggers and the Datastream streams.'), kind: 'text', placeholder: 'us-central1' },
          { key: 'bigquery_location', label: t('BigQuery location'), hint: t('Of the scheduled queries: us, eu, or a region.'), kind: 'text', placeholder: 'us' },
          { key: 'cloud_run_jobs', label: t('Cloud Run jobs'), hint: t('Executions and the Cloud Scheduler trigger of each job.'), kind: 'toggle' },
          { key: 'bigquery_freshness', label: t('Table freshness'), hint: t('The tables listed below (table metadata only).'), kind: 'toggle' },
          { key: 'scheduled_queries', label: t('Scheduled queries'), hint: t('State of the last run, never the SQL.'), kind: 'toggle' },
          { key: 'datastream', label: 'Datastream', hint: t('State of the streams, and their lag (Cloud Monitoring).'), kind: 'toggle' },
          { key: 'bigquery_cost', label: t('BigQuery cost'), hint: t('Estimate from the bytes billed by the jobs (jobs.list, free). Needs bigquery.jobs.listAll to see everyone’s jobs.'), kind: 'toggle' },
          { key: 'price_per_tib', label: t('Price'), hint: t('On-demand price per TiB billed.'), kind: 'number', unit: '$ / TiB', step: 0.01 },
          { key: 'poll_seconds', label: t('Poll every'), hint: t('BigQuery jobs are read every 5 minutes whatever this is.'), kind: 'number', unit: t('seconds') },
          { key: 'slow_factor', label: t('Job slow'), hint: t('A Cloud Run job running longer than this multiple of its median duration is flagged (formerly [alerts] data_slow_factor).'), kind: 'number', unit: t('× median'), step: 0.5 },
          { key: 'stream_lag_minutes', label: t('Stream lagging'), hint: t('A Datastream stream further behind is flagged (formerly [alerts] data_stream_lag_minutes).'), kind: 'number', unit: t('minutes') },
          { key: 'cost_spike_factor', label: t('Cost spike'), hint: t('Today’s estimated cost above this multiple of the 14-day median is flagged (formerly [alerts] data_cost_spike_factor).'), kind: 'number', unit: t('× median'), step: 0.5 },
          {
            key: 'tables', label: t('Tables'), kind: 'table', blank: { table: '', max_age_hours: 26 },
            hint: t('Late when not modified for longer than Max age. Fed by: the Cloud Run job that writes it; red when that job failed.'),
            columns: [
              { key: 'table', label: t('Table'), kind: 'text', placeholder: 'project.dataset.table' },
              { key: 'max_age_hours', label: t('Max age (h)'), kind: 'number', width: '100px' },
              { key: 'fed_by', label: t('Fed by'), kind: 'text', optional: true, placeholder: t('Cloud Run job'), width: '160px' },
            ],
          },
        ],
      },
    ],
  },
} satisfies BlockDef<GcpData>

/** The BigQuery cost block: the same source as Data (`source`), its cost only. */
export const bqcost = {
  id: 'bqcost' as const,
  label: t('BigQuery cost'),
  category: 'Data',
  source: 'data',
  help: {
    purpose: t('What BigQuery queries cost today, and whether it is a spike.'),
    points: [
      t('Today versus the median of the last 14 days ([data] cost_spike_factor), this month, and the bytes billed.'),
      t('The most expensive jobs of the day: statement type, tables read, service account. People are never named.'),
      t('An estimate: bytes billed × the on-demand price ([data] price_per_tib), no discounts, no storage.'),
    ],
    source: t('BigQuery jobs.list (free, without the job configuration, so without SQL), every 5 min, or now with ↻.'),
  },
  badge,
} satisfies BlockDef<GcpData>
