# Brief interface system

Brief should feel like a private macOS financial instrument: calm, compact, and evidence-first. `src/styles.css` and shared React components are the implementation source of truth; this document records the stable design rules.

## Principles

- Prefer clear hierarchy and dense, reusable rows over decorative dashboard chrome.
- Reserve color for selection, status, category identity, and financial meaning.
- Show missing or incomplete evidence explicitly. Never imply precision with placeholders or color alone.
- Use card-based navigation rather than horizontal top tabs.
- Keep headings concise. Put necessary scope, provenance, and instructions beside the relevant value, control, or state instead of beneath the heading.
- The supported interface is desktop-only at widths of 860 px and above.

## Foundations

Shared tokens live in `:root` in `src/styles.css`.

| Role                | Tokens                                                                                        |
| ------------------- | --------------------------------------------------------------------------------------------- |
| Canvas and surfaces | `--background`, `--surface`, `--popup-surface`, `--control-background`                        |
| Text                | `--text-primary`, `--text-secondary`, `--text-muted`                                          |
| Interaction         | `--accent`, `--accent-soft`, `--control-border`                                               |
| Financial status    | `--positive`, `--warning`, `--negative`                                                       |
| Data categories     | `--data-1` through `--data-7`                                                                 |
| Typography          | `--type-meta`, `--type-body`, `--type-section`, `--type-page`, `--type-metric`, `--type-hero` |
| Geometry            | `--radius-control`, `--radius-card`, `--card-padding`, `--control-height`, `--row-height`     |

The system sans stack is the default. `--font-editorial` is used for the brand and page-level editorial headings. Financial values use tabular numerals.

Cards use the shared translucent surface, subtle border, 16 px radius, and 24 px standard padding. Controls use an 8 px radius. The standard row height is 64 px. The page is centered to a 1560 px maximum, with a fixed 184 px sidebar; page horizontal padding decreases from 48 px to 32 px below 1050 px.

## Color

- Green and red represent favorable and unfavorable financial direction only when a signed value or text also communicates the meaning.
- The seven data colors identify categories and series; they are not gain/loss colors.
- Asset categories use their semantic aliases from `src/styles.css`.
- Unsupported or unavailable data uses the muted text treatment, not a success or failure color.
- Do not add page-local colors when a semantic token already exists.

## Components

Use the shared components before creating page-specific variants:

- `Card` for surfaces
- `SectionHeading` for card titles and a right-side action
- `Metric` for label/value/detail hierarchy
- `Button` for actions and icon controls
- `RangeSelector` for compact date ranges
- `EmptyState` for unavailable, empty, or blocked content
- shared account, activity, position, and sale rows for financial lists

Icon-only controls require an accessible name. Stateful controls expose the appropriate `aria-pressed`, `aria-expanded`, `aria-busy`, or disabled state. Wide financial tables scroll inside their cards.

Tooltips are for compact evidence attached to chart or data marks, not for ordinary labels or controls. Interactive eligibility or terms content belongs in a click-to-open popover. Popup surfaces use the shared elevated material and must remain keyboard-dismissible.

## Layout

`WorkspaceHeader` owns the route title, snapshot or market status, and route actions. Related cards use 16 px gaps; major Home groups use 24 px gaps. At supported narrower widths, primary overview grids collapse rather than introducing a mobile navigation model.

Do not duplicate visible page contents in subtitles. Use breadcrumbs for query-backed scope such as the selected account, ticker, category, or date range.

## Charts and financial data

- Range controls stay next to the value or comparison they affect.
- Use explicit labels for saved, delayed, estimated, incomplete, and unavailable data.
- Canonical bar history, latest observations, comparisons, and cash-flow markers remain visually and semantically distinct.
- Chart colors come from shared CSS tokens.
- Every chart has an accessible label; hover-only information must have a keyboard-accessible equivalent.
- Never present interpolated or estimated display values as observed financial evidence.

The patched Liveline behavior and upgrade constraints are documented in [../docs/liveline-patch-review.md](../docs/liveline-patch-review.md).

## Interaction and accessibility

- Preserve native button, link, form, and dialog semantics.
- Focus uses a visible two-pixel outline; full-row controls may inset it.
- Support keyboard operation for every interactive path.
- Keep direct feedback brief and interruptible. Avoid ornamental page transitions.
- Respect `prefers-reduced-motion`; `src/styles.css` reduces animation and transition duration globally.
- Loading, empty, error, hover, active, focus, and disabled states must remain distinguishable.
- Hidden scrollbar chrome must not remove scrolling or keyboard access.

Settings → Design renders synthetic previews of the current tokens and shared components. Its controls are local previews and do not persist financial or display state.
