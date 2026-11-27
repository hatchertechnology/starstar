import { describe, expect, it } from 'vitest'
import { fileDetailsRoute, filePath, folderQuery, parseFolderQuery, printerPath, splitPath, thumbnailPath } from '~/utils/paths'

describe('paths', () => {
  it('encodes each segment', () => {
    expect(filePath('mk4', 'usb', ['My Parts', 'café #1?%.bgcode'])).toBe('/api/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode')
    expect(filePath('mk4', 'usb', ['a.bgcode'], 'print')).toBe('/api/printers/mk4/files/usb/a.bgcode/print')
    expect(filePath('mk4', 'usb', [])).toBe('/api/printers/mk4/files/usb')
    expect(splitPath('/a//b/')).toEqual(['a', 'b'])
  })
  it('encodes printer names and joins rest', () => {
    expect(printerPath('my printer/1', 'job', '42', 'pause')).toBe('/api/printers/my%20printer%2F1/job/42/pause')
    expect(printerPath('mk4')).toBe('/api/printers/mk4')
  })
  it('builds details and thumbnail paths', () => {
    expect(fileDetailsRoute('mk4', 'usb', ['My Parts', 'café #1?%.bgcode'])).toBe('/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode')
    expect(thumbnailPath('mk4', 'usb', ['My Parts', 'café #1?%.bgcode'])).toBe('/api/printers/mk4/thumbnails/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode')
  })
  it('round-trips a folder through the query', () => {
    expect(folderQuery('usb', [])).toEqual({ storage: 'usb' })
    expect(folderQuery('usb', ['a', 'b c'])).toEqual({ storage: 'usb', dir: 'a/b c' })
    expect(parseFolderQuery(folderQuery('usb', ['a', 'b c']))).toEqual({ storage: 'usb', segments: ['a', 'b c'] })
    expect(parseFolderQuery({})).toEqual({ storage: undefined, segments: [] })
  })
})
