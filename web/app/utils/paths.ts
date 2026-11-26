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
