# Liveline patch

Brief uses `liveline@0.0.7` through the pnpm patch declared in `pnpm-workspace.yaml` and locked in `pnpm-lock.yaml`.

```text
patches/liveline@0.0.7.patch
```

The patch modifies both runtime formats and both declaration outputs. Keep those copies synchronized.

## Behavior Brief relies on

- `breakBefore` prevents interpolation and drawing across known gaps or feed boundaries.
- Window filtering can insert a presentation-only left-boundary point for sparse series.
- `valueTime` and `endTime` separate an observation endpoint from wall-clock time and support fixed historical windows.
- `minValueRange` places a lower bound on the displayed numerical span.
- Series `dash` and `currentLine` control comparison and current-value presentation.
- `continuous={false}` allows settled charts to stop requesting frames.
- Accessible markers are positioned from chart coordinates and remain filtered against `endTime` for historical views.
- The primary series can retain an endpoint badge in multi-series mode.
- Candles support an optional display `width`, fixed-window endpoints, quote markers, reference lines, and center-based hover timestamps.
- Canvas fonts match the app's system typography.

Brief owns its external range controls and comparison key. Do not expand the patch for unused Liveline controls.

## Upgrade checklist

Before changing the patch or upgrading Liveline:

1. Compare each relied-on behavior above with the candidate upstream API.
2. Remove patch sections that upstream now supplies; rebase only the remaining behavior.
3. Apply equivalent runtime changes to ESM and CommonJS outputs.
4. Apply equivalent type changes to both declaration outputs.
5. Run:

   ```sh
   pnpm install
   pnpm exec vitest run src/components/charts.test.tsx src/lib/holding-prices.test.ts
   pnpm build
   ```

6. Confirm `pnpm-lock.yaml` contains the new patch hash.
7. Restart Vite after the installed patch changes; a running server can retain the previous package realpath.

The focused tests cover fixed-window marker filtering and renderer-side bar/candle transformations. They do not prove canvas pixel output, pointer interaction, or macOS WebView frame pacing.
