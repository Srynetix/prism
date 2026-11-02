import { describe, expect, it } from 'vitest'
import { pipelineLabel } from './pipeline'
import type { Pipeline, Stage } from './types'

const stage = (name: string, status: Stage['status']): Stage => ({ name, status, jobs: 1, finished: 1, failed: 0, failed_jobs: [], manual_jobs: [], started_at: null, finished_at: null })
const pipeline = (status: Pipeline['status'], stages: Stage[]): Pipeline => ({ id: 1, repo: null, project: null, target: 'main', status, url: '', updated_at: '', failed_jobs: [], stages })

describe('pipelineLabel', () => {
  it('says canceled, at the stage where it stopped, not passed', () => {
    expect(pipelineLabel(pipeline('canceled', [stage('build', 'success'), stage('test', 'canceled'), stage('deploy', 'canceled')]), 0)).toEqual({ text: 'canceled · test', tone: 'cx' })
  })

  it('says canceled when the forge says so and no stage does', () => {
    expect(pipelineLabel(pipeline('canceled', [stage('build', 'success'), stage('deploy', 'skipped')]), 0)).toEqual({ text: 'canceled', tone: 'cx' })
  })

  it('still says passed for a successful pipeline', () => {
    expect(pipelineLabel(pipeline('success', [stage('build', 'success')]), 0)).toEqual({ text: 'passed', tone: 'ok' })
  })
})
