import { describe, expect, it } from 'vitest'
import { tail } from './fix-context'

describe('tail', () => {
  it('keeps the last non-empty lines, without colors nor GitLab sections', () => {
    const log = 'section_start:1:step_script\r\x1b[0K\x1b[32;1m$ pytest\x1b[0;m\nok\n\n\x1b[31mFAILED tests/test_a.py::test_x\x1b[0m\nsection_end:2:step_script\r\x1b[0K\nERROR: Job failed: exit code 1\n'
    expect(tail(log, 2)).toBe('FAILED tests/test_a.py::test_x\nERROR: Job failed: exit code 1')
    expect(tail(log, 10).split('\n')[0]).toBe('$ pytest')
  })
  it('keeps a progress bar’s last state only', () => {
    expect(tail('10%\r50%\r100%\ndone', 5)).toBe('100%\ndone')
  })
})
