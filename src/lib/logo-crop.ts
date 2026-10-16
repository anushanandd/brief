import { useEffect, useState } from 'react'

/** How to zoom and shift a logo so its artwork fills a circle; offsets are percents. */
export type LogoCrop = { scale: number; x: number; y: number }

const MAX_ZOOM = 2
/** A pixel this unlike the padding is artwork; fainter differences are compression noise. */
const ARTWORK_DIFFERENCE = 96

/**
 * The crop for a square logo served on a light, neutral margin, such as a brand mark drawn on
 * white. A logo whose corners are transparent, mismatched, or colored (a brand background)
 * fills its circle as served and gets no crop.
 */
export function logoCrop(pixels: Uint8ClampedArray, size: number): LogoCrop | undefined {
  const at = (x: number, y: number) => (y * size + x) * 4
  const corners = [at(0, 0), at(size - 1, 0), at(0, size - 1), at(size - 1, size - 1)]
  const [r, g, b, a] = pixels.slice(corners[0], corners[0] + 4)
  const differs = (index: number) =>
    Math.abs(pixels[index] - r) + Math.abs(pixels[index + 1] - g) + Math.abs(pixels[index + 2] - b)
  const light = Math.min(r, g, b) >= 235 && Math.max(r, g, b) - Math.min(r, g, b) <= 12
  if (a < 250 || !light || corners.some((index) => pixels[index + 3] < 250 || differs(index) >= 24))
    return undefined
  let minX = size
  let minY = size
  let maxX = -1
  let maxY = -1
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++)
      if (differs(at(x, y)) >= ARTWORK_DIFFERENCE) {
        minX = Math.min(minX, x)
        maxX = Math.max(maxX, x)
        minY = Math.min(minY, y)
        maxY = Math.max(maxY, y)
      }
  if (maxX < 0) return undefined
  // Overscan about a pixel each side so the artwork's antialiased edge never shows.
  const overscan = Math.max(2, Math.round(size / 64))
  // Artwork that already reaches the edges needs no crop.
  if (Math.min(minX, minY, size - 1 - maxX, size - 1 - maxY) <= overscan) return undefined
  const box = Math.max(maxX - minX + 1, maxY - minY + 1) - overscan * 2
  const scale = Math.min(size / box, MAX_ZOOM)
  const offset = (low: number, high: number) => ((low + high + 1) / 2 / size - 0.5) * 100 * scale
  return { scale, x: -offset(minX, maxX), y: -offset(minY, maxY) }
}

/** Analyses by logo URL: each logo is measured once per session. */
const crops = new Map<string, Promise<LogoCrop | undefined>>()
const settled = new Map<string, LogoCrop | undefined>()

function analyze(src: string) {
  const existing = crops.get(src)
  if (existing) return existing
  const measured = new Promise<LogoCrop | undefined>((resolve) => {
    // A separate probe reads the pixels, so a host without CORS never blocks the shown image.
    const probe = new Image()
    probe.crossOrigin = 'anonymous'
    probe.addEventListener('error', () => resolve(undefined), { once: true })
    probe.addEventListener(
      'load',
      () => {
        try {
          const size = probe.naturalWidth
          if (!size || size !== probe.naturalHeight) return resolve(undefined)
          const canvas = document.createElement('canvas')
          canvas.width = canvas.height = size
          const context = canvas.getContext('2d', { willReadFrequently: true })
          if (!context) return resolve(undefined)
          context.drawImage(probe, 0, 0)
          resolve(logoCrop(context.getImageData(0, 0, size, size).data, size))
        } catch {
          resolve(undefined)
        }
      },
      { once: true },
    )
    probe.src = src
  }).then((crop) => {
    settled.set(src, crop)
    return crop
  })
  crops.set(src, measured)
  return measured
}

/** The cached crop for a remote logo, measured on first use. */
export function useLogoCrop(src: string | undefined) {
  const remote = src?.startsWith('https://') ? src : undefined
  const [crop, setCrop] = useState(() => (remote ? settled.get(remote) : undefined))
  useEffect(() => {
    if (!remote) return setCrop(undefined)
    if (settled.has(remote)) return setCrop(settled.get(remote))
    let active = true
    void analyze(remote).then((result) => active && setCrop(result))
    return () => {
      active = false
    }
  }, [remote])
  return crop
}
