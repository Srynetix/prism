---
title: Data pipelines (GCP)
group: Watch the system
blocks: data, bqcost
related: settings, infra, todo, ref-settings
---
The **Data** and **BigQuery cost** blocks watch your data pipelines on Google Cloud: is a job broken, a table late, a stream lagging, or today's BigQuery bill drifting? Prism reads metadata only, and never a row of your data.

## Turning it on
The data blocks are off by default. Prism reads GCP with your `gcloud` login, read-only, so run `gcloud auth login` first.

1. Open **Settings › Infra › Data (GCP)** and switch on **GCP access** (`[data] enabled`). A check next to the switch tells you whether your gcloud login works.
2. List the GCP project ids to read in **Projects**, and set the **Region** of your Cloud Run jobs and Datastream streams, and the **BigQuery location** of your scheduled queries.
3. Add the tables you expect to be updated in **Tables** (see below).
4. Save with `⌘S`, then click **Restart Prism**: the Data section is read at launch.
5. Add the **Data** and **BigQuery cost** blocks to a page with **+ Block** in **Edit layout**, or start from a pack's Data template in the pages wizard.

[Open Settings › Infra › Data (GCP)](prism:settings/data)

Each kind of check has its own switch, all on by default: **Cloud Run jobs**, **Table freshness**, **Scheduled queries**, **Datastream** and **BigQuery cost**.

> [!NOTE]
> Prism reads metadata only: never rows, table schemas, SQL, log lines or an execution's message (only its exit code), and never the name of the person who ran a query. All the calls are free: no BigQuery query is run.

## The Data block
The title sums things up: how many items are broken, how many are drifting, how many are fine. By default the block shows **Problems** only; click **All** to see everything. In short mode, it lists just the problems.

**Cloud Run jobs.** Each job shows its schedule, a small history of its last executions, the duration of the last run and a state pill:
- **failed** (or **failed ×3** for failures in a row), with the exit code in the tooltip.
- **run missed**: Cloud Scheduler could not start the job.
- **2.5× median**: the run took, or is still taking, far longer than usual. The threshold is `[data] slow_factor` (2 by default).
- **running**, **cancelled**, **never run**, or **ok**.

Click a job to unfold it: why it is flagged, the last run, its duration against the median, its trigger and next run, when it was deployed, and links to **Executions** and **Logs** in the GCP console.

**BigQuery freshness.** The tables you declared, with their age against their maximum age. A table not modified for longer than its **Max age** is late, in orange. It turns red when the Cloud Run job that feeds it failed.

**Scheduled queries.** Shown when something is wrong (a failed or cancelled last run), or under **All**.

**Datastream.** Each stream shows its source and destination, its replication lag and its state. A failed stream is red; a stream behind by more than `[data] stream_lag_minutes` (5 by default) is orange. Click it to see its lag over the last 6 hours and a link to the Datastream console.

## Declaring tables
Prism never guesses which tables matter. In **Settings › Infra › Data (GCP) › Tables**, add one row per table:

| Column | Meaning |
|---|---|
| Table | `project.dataset.table` |
| Max age (h) | late beyond this many hours |
| Fed by | the Cloud Run job that writes it (optional) |

With **Fed by** set, a late table whose job failed shows in red: you see the cause, not just the symptom.

## The BigQuery cost block
This block estimates what BigQuery queries cost: the bytes billed multiplied by the on-demand price (`[data] price_per_tib`, 6.25 by default). It is an estimate, not your bill: no discounts, no storage.

- **Today**, against the median of the last 14 days. Above `[data] cost_spike_factor` (3 by default) times that median, it is flagged as a spike.
- **This month**, and the bytes billed today, with a bar per day for the last 14 days.
- **Most expensive today**: the statement type, the tables read and the service account. A query run by a person shows as "a person": nobody is named.

> [!TIP]
> To see everyone's jobs, not just your own, your account needs the `bigquery.jobs.listAll` permission.

## Refreshing and alerts
The Data block is read every `[data] poll_seconds` (60 s by default); BigQuery jobs, for the cost, every 5 minutes. The ↻ button in either block reads GCP again right away, cost included.

What is broken shows up in **Alerts** and sends one notification. What only drifts (a slow job, a late table, a lagging stream, a cost spike) stays in **Alerts** without notifying.

All three thresholds live in **Settings › Interface › Alerts & sound** and apply as soon as you save.

[Open Settings › Interface › Alerts & sound](prism:settings/alerts)
