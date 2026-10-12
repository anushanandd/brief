import { expect, it } from 'vitest'

import { logoCrop } from './logo-crop'

/** A square image of `background` with a `box` of color from `from` to `to`, inclusive. */
function image(
  size: number,
  background: number[],
  box?: { from: number; to: number; color: number[] },
) {
  const pixels = new Uint8ClampedArray(size * size * 4)
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      const inside = box && x >= box.from && x <= box.to && y >= box.from && y <= box.to
      pixels.set(inside ? box.color : background, (y * size + x) * 4)
    }
  return pixels
}

const white = [255, 255, 255, 255]
const blue = [0, 111, 209, 255]

it('zooms a logo on a white margin until its artwork fills the circle', () => {
  const pixels = image(128, white, { from: 18, to: 109, color: blue })
  // Faint compression noise in the margin is not artwork.
  pixels.set([251, 254, 250, 255], (111 * 128 + 111) * 4)
  const crop = logoCrop(pixels, 128)!
  expect(crop.scale).toBeCloseTo(128 / 88, 5)
  expect(crop.x).toBeCloseTo(0, 5)
  expect(crop.y).toBeCloseTo(0, 5)
})

it('centers artwork that sits off center', () => {
  const crop = logoCrop(image(100, white, { from: 30, to: 89, color: blue }), 100)!
  expect(crop.x).toBeLessThan(0)
  expect(crop.y).toBeCloseTo(crop.x, 5)
})

it('leaves brand backgrounds, transparent logos, and full-bleed art as served', () => {
  const brand = [0, 84, 170, 255]
  expect(logoCrop(image(64, brand, { from: 20, to: 43, color: white }), 64)).toBeUndefined()
  expect(logoCrop(image(64, [0, 0, 0, 0], { from: 8, to: 55, color: blue }), 64)).toBeUndefined()
  expect(logoCrop(image(64, white, { from: 1, to: 62, color: blue }), 64)).toBeUndefined()
  expect(logoCrop(image(64, white), 64)).toBeUndefined()
})
