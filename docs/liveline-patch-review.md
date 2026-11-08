# Liveline patch

Brief uses `liveline@0.0.7` through the pnpm patch declared in `pnpm-workspace.yaml` and locked in `pnpm-lock.yaml`.

```text
patches/liveline@0.0.7.patch
```

The patch changes only the ES module build (`dist/index.js`) and its types (`dist/index.d.ts`): Vite, Vitest, and TypeScript all resolve Liveline through its `import` entry, so the CommonJS build is never loaded. If something ever `require`s Liveline, it gets the unpatched build.

Liveline draws the value histories (`src/components/charts.tsx`), which rely on the patch's multi-series parts (series `dash`, the primary series' badge in multi-series mode), and the streaming Holdings chart (`src/components/holding-chart.tsx`).

## Behavior Brief relies on

- `breakBefore` prevents interpolation and drawing across known gaps or feed boundaries.
- Window filtering can insert a presentation-only left-boundary point for sparse series.
- `valueTime` and `endTime` separate an observation endpoint from wall-clock time and support fixed historical windows.
- `minValueRange` places a lower bound on the displayed numerical span.
- Series `dash` and `currentLine` control comparison and current-value presentation.
- `continuous={false}` allows settled charts to stop requesting frames.
- A chart mounted with data appears fully drawn; the reveal plays only when data arrives after an empty or loading state.
- Accessible markers are dots on the line, positioned from chart coordinates and filtered against `endTime` for historical views.
- The primary series can retain an endpoint badge in multi-series mode.
- Candles support an optional display `width`, fixed-window endpoints, quote markers, reference lines, and center-based hover timestamps.
- Canvas fonts match the app's system typography.

Brief owns its external range controls and comparison key. Do not expand the patch for unused Liveline controls.

## Upgrade checklist

Before changing the patch or upgrading Liveline:

1. Compare each relied-on behavior above with the candidate upstream API.
2. Remove patch sections that upstream now supplies; rebase only the remaining behavior.
3. Edit only `dist/index.js` and `dist/index.d.ts` (`pnpm patch liveline@0.0.7`, then `pnpm patch-commit`).
4. Run:

   ```sh
   pnpm install
   pnpm exec vitest run src/components/charts.test.tsx src/lib/holding-prices.test.ts
   pnpm build
   ```

5. Confirm `pnpm-lock.yaml` contains the new patch hash.
6. Restart Vite after the installed patch changes; a running server can retain the previous package realpath.

The focused tests cover fixed-window marker filtering and renderer-side bar/candle transformations. They do not prove canvas pixel output, pointer interaction, or macOS WebView frame pacing.
