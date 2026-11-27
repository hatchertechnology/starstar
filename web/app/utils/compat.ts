import type { PrintMeta, PrinterKind } from '~/types/api'

function family(model: string): PrinterKind | null {
  const m = model.toUpperCase()
  if (m.startsWith('MK4')) return 'mk4'
  if (m.startsWith('MINI')) return 'mini'
  return null
}

/**
 * A warning when a file was sliced for another printer family or nozzle size,
 * or null when they match or there is nothing to compare.
 */
export function slicedMismatch(
  meta: PrintMeta | null | undefined,
  kind: PrinterKind,
  nozzle: number | null | undefined,
  printer: string,
): string | null {
  const model = meta?.printer_model ?? null
  const fileNozzle = meta?.nozzle_diameter ?? null
  const fileFamily = model ? family(model) : null
  const familyDiffers = fileFamily !== null && fileFamily !== kind
  const nozzleDiffers = fileNozzle != null && nozzle != null && Math.abs(fileNozzle - nozzle) > 0.001
  if (!familyDiffers && !nozzleDiffers) return null
  const sliced = `This file was sliced${model ? ` for ${model}` : ''}${fileNozzle != null ? ` with a ${fileNozzle} mm nozzle` : ''}.`
  const target = `${printer} is a ${kind.toUpperCase()}${nozzle != null ? ` with a ${nozzle} mm nozzle` : ''}.`
  return `${sliced} ${target}`
}
