import { describe, expect, it } from 'vitest'
import { baseName, dirName, tildePath } from './platform'

describe('paths of every system', () => {
  it('names the last part, with / or \\ between the parts', () => {
    expect(baseName('/Users/me/work/api')).toBe('api')
    expect(baseName('C:\\Users\\me\\work\\api')).toBe('api')
    expect(baseName('C:/Users/me/work/api/')).toBe('api')
    expect(baseName('api')).toBe('api')
  })

  it('cuts the folder off', () => {
    expect(dirName('/Users/me/.config/prism/config.toml')).toBe('/Users/me/.config/prism')
    expect(dirName('C:\\Users\\me\\AppData\\Roaming\\prism\\config.toml')).toBe('C:\\Users\\me\\AppData\\Roaming\\prism')
    expect(dirName('config.toml')).toBe('')
  })

  it('writes the home folder ~', () => {
    expect(tildePath('/Users/me/work')).toBe('~/work')
    expect(tildePath('/home/me/work')).toBe('~/work')
    expect(tildePath('C:\\Users\\me\\work')).toBe('~\\work')
  })
})
