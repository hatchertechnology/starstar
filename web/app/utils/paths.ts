import type { LocationQuery } from 'vue-router'

const enc = encodeURIComponent

/** `/api/printers/<name>/<rest…>`, each segment percent-encoded. */
export function printerPath(name: string, ...rest: string[]): string {
  return ['/api/printers', enc(name), ...rest.map(enc)].join('/')
}

/** Path of a file or folder on a printer's storage. */
export function filePath(name: string, storage: string, segments: string[], suffix?: 'print'): string {
  const parts = ['/api/printers', enc(name), 'files', enc(storage), ...segments.map(enc)]
  if (suffix) parts.push(suffix)
  return parts.join('/')
}

export function splitPath(path: string): string[] {
  return path.split('/').filter(Boolean)
}

/** API path of a file's preview image. */
export function thumbnailPath(name: string, storage: string, segments: string[]): string {
  return ['/api/printers', enc(name), 'thumbnails', enc(storage), ...segments.map(enc)].join('/')
}

/** UI route of a print file's details page. */
export function fileDetailsRoute(name: string, storage: string, segments: string[]): string {
  return ['/printers', enc(name), 'files', enc(storage), ...segments.map(enc)].join('/')
}

/** Query that reopens a folder in the file browser. */
export function folderQuery(storage: string, segments: string[]): { storage: string; dir?: string } {
  return segments.length ? { storage, dir: segments.join('/') } : { storage }
}

/** Reads a folder back from the query written by `folderQuery`. */
export function parseFolderQuery(q: LocationQuery): { storage?: string; segments: string[] } {
  const one = (v: LocationQuery[string] | undefined) => (Array.isArray(v) ? v[0] : v) ?? undefined
  const storage = one(q.storage)
  const dir = one(q.dir)
  return { storage: storage || undefined, segments: dir ? splitPath(dir) : [] }
}

// legacy: removed in Task 12 (part page)
export function libraryPath(id?: number, suffix?: 'preview' | 'download'): string {
  return ['/api/library', ...(id === undefined ? [] : [String(id)]), ...(suffix ? [suffix] : [])].join('/')
}

export function partsPath(id?: number, suffix?: 'revisions' | 'setups' | 'files' | 'match'): string {
  return ['/api/parts', ...(id === undefined ? [] : [String(id)]), ...(suffix ? [suffix] : [])].join('/')
}

export function setupsPath(id: number, suffix?: 'revisions'): string {
  return ['/api/setups', String(id), ...(suffix ? [suffix] : [])].join('/')
}

export function filesPath(id: number, suffix?: 'preview' | 'download'): string {
  return ['/api/files', String(id), ...(suffix ? [suffix] : [])].join('/')
}
