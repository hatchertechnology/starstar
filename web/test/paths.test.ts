import { describe, expect, it } from 'vitest'
import { partsPath, setupsPath, filesPath, fileDetailsRoute, filePath, folderQuery, parseFolderQuery, printerPath, splitPath, thumbnailPath } from '~/utils/paths'

describe('paths', () => {
  it('builds parts paths', () => {
    expect(partsPath()).toBe('/api/parts')
    expect(partsPath(7, 'revisions')).toBe('/api/parts/7/revisions')
    expect(partsPath(7, 'setups')).toBe('/api/parts/7/setups')
    expect(partsPath(7, 'files')).toBe('/api/parts/7/files')
    expect(partsPath(7, 'match')).toBe('/api/parts/7/match')
    expect(setupsPath(2)).toBe('/api/setups/2')
    expect(setupsPath(2, 'revisions')).toBe('/api/setups/2/revisions')
    expect(filesPath(8)).toBe('/api/files/8')
    expect(filesPath(8, 'preview')).toBe('/api/files/8/preview')
    expect(filesPath(8, 'download')).toBe('/api/files/8/download')
  })
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
