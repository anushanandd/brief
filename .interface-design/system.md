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

| Role                | Tokens                                                                                                              |
| ------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Canvas and surfaces | `--background`, `--surface`, `--popup-surface`, `--control-background`                                              |
| Text                | `--text-primary`, `--text-secondary`, `--text-muted`                                                                |
| Interaction         | `--accent`, `--accent-soft`, `--control-border`                                                                     |
| Financial status    | `--positive`, `--warning`, `--negative`                                                                             |
| Data categories     | `--data-1` through `--data-7`                                                                                       |
| Typography          | `--type-meta`, `--type-body`, `--type-section`, `--type-page`, `--type-metric`, `--type-hero`                       |
| Geometry            | `--radius-control`, `--radius-card`, `--card-padding`, `--companion-card-width`, `--control-height`, `--row-height` |

The macOS SF Pro system stack is used for body text, controls, metrics, and code-like text. Editorial titles use Iowan Old Style with Palatino, Book Antiqua, Georgia, and generic serif fallbacks. The Brief wordmark and page titles use the regular-weight editorial face. Financial values use tabular numerals.

Interface icons use Hugeicons Free Stroke Rounded through `src/components/icons.tsx`, with a 1.75 default stroke width and sizes set by their existing controls and marks.

Cards use the shared translucent surface, subtle border, 16 px radius, and 24 px standard padding. Standard companion cards use the Account details width: 27.5% of the available row with a 280 px floor. Rows with two companions reserve that width for each and let the primary card fill the remainder. Controls use an 8 px radius. The standard row height is 64 px. The page is centered to a 1560 px maximum, with a fixed 256 px sidebar; page horizontal padding decreases from 48 px to 32 px below 1050 px. The sidebar follows shadcn’s header/content/group/menu/footer composition with uniform full-width rows, quiet section labels, and no darker overlay than the app canvas. Durable subpages are grouped beneath the primary workspace menu. The sidebar links to the Accounts workspace but does not list individual accounts. The Accounts workspace account selector excludes the saved spending account and keeps every remaining account in one horizontally scrollable row; selectors show the display name, value, and signed range change without provider or institution metadata.

The native window opens maximized with macOS decorations and an overlaid, hidden title bar so its corners remain rounded. Its traffic-light controls are hidden. The white semibold Brief wordmark uses page-title typography, aligns vertically with page titles, and shares the Workspace sidebar label’s left edge.

The app frame and background stay fixed to the window. Longer routes scroll inside the main content pane without moving the frame; Activity keeps its results and filters as the card scroll regions. Changing routes resets the main content pane to its top.

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
- `Metric` for concise label/value pairs only; do not add secondary metric details
- `Button` for actions and icon controls
- `ChartRangeSelect` for graph and date ranges; it uses a compact transparent trigger and concise names such as Week, Month, Quarter, and Year. Use `RangeSelector` for compact non-date choices
- `EmptyState` for unavailable, empty, or blocked content
- shared account, activity, position, and sale rows for financial lists

Icon-only controls require an accessible name. Stateful controls expose the appropriate `aria-pressed`, `aria-expanded`, `aria-busy`, or disabled state. Wide financial tables scroll inside their cards.
Activity ledger rows and filters show account type marks and distinct category marks beside their names. Actual merchant logos are preferred when enabled; missing or disabled merchant logos fall back to the native transaction or category mark, never generated initials. Native transaction marks determine row icons for transfers, interest, dividends, payments, refunds, and fees; known spending categories use meaningful distinct icons. Activity dates and pending labels use primary text. Activity search stays a single-line control. Borderless remove controls for selected account, category, method, date range, and analytics scope sit to its right in the ledger toolbar, with muted scope labels and normal value text. The Activity search and filter search have transparent surfaces and no focus outline; their caret and adjacent icon or key hint change provide focus feedback. Search, the date-range menu, and Download share the ledger toolbar, with the menu immediately left of Download. A persistent Filters card sits to the right of the ledger with Accounts, Categories, and Methods in one scrollable column and Clear all beside its heading. At narrow desktop widths the cards stack to keep the ledger usable. F focuses the card's filter search; arrow keys and Return choose available values, Escape clears the search and leaves filter selection, C clears all filters outside text entry. After a filter choice, focus moves to the filter region so page shortcuts work again. The date-range menu supports native keyboard selection; Custom reveals the two-thumb day-range slider immediately below the toolbar, with each thumb keyboard adjustable. Presets apply immediately and the selected date range appears as a removable toolbar tag. Frequent keyboard actions do not animate. Filter marks share one compact size, and options omit Enter glyphs. Search text and the selected date range limit available filter options. Each option group also respects the other selected facets, while chosen options leave their groups until their tags are removed. Number keys open sidebar pages outside text entry. Left and right arrows change the selected account, analytics chart, holding, or spending period. Location shows the method’s colored mark beside city and state; when location is missing it shows the method label and filters by method. An icon beside a supplied place reveals its street address in a tooltip. Brokerage activity uses its own method mark. There is no separate Method column; method filtering remains in the Filters card. Missing values use an unavailable marker. Description, account, category, location, and date cells apply filters when clicked or activated by keyboard. Description and location use search; the full description remains in the description control's accessible name without a hover tooltip. Merchant websites use a separate link beside the description. Account, category, location, date, and amount columns size from all loaded activity rows, including rows outside the current filters, so filtering does not move dividers. Category and location use compact width limits for unusually long labels. Description takes the remaining width with a 180 px minimum; its displayed titles show at most five words and truncate when needed. The Activity page stays fixed to the window; results, filters, and overflowing toolbar tags scroll inside their cards. Wide results scroll inside the ledger card. Row dates are centered and omit the visible year and time. Negative activity amounts use the negative color token without bold weight. The ledger uses aligned vertical column dividers and horizontal row separators. Column headings use text only, matching the row text color and size in bold; description rows use regular weight and white text.
Cards with vertical overflow show a bottom chevron on a faded, borderless circular surface while more content remains below; it disappears at the end of the scroll region.

Tooltips are for compact evidence attached to chart or data marks, not for ordinary labels or controls. Interactive eligibility or terms content belongs in a click-to-open popover. Popup surfaces use the shared elevated material and must remain keyboard-dismissible.

## Layout

`WorkspaceHeader` owns the route title, snapshot or market status, and route actions. Related cards use 16 px gaps; major Home groups use 24 px gaps. At supported narrower widths, overview grids collapse rather than introducing a mobile navigation model; three-card rows stack at 1350 px and below so their primary card does not become unusably narrow. Account details, account Recent and Expected activity, Spending Metrics, Platinum benefits and Expected activity, and Home Accounts and Recent activity share the standard companion width; Home Holdings fills the remaining column. Individual investment accounts omit Expected activity, keep Recent activity in the right companion column, and let Holdings and Realized sales fill the wider primary column. Cash accounts place Recent activity in the wide left column and Expected activity in the right companion column.

Do not duplicate visible page contents in subtitles. Use breadcrumbs for query-backed scope such as the selected account, ticker, category, or date range.

## Charts and financial data

- Range controls stay in the chart header’s top-right corner.
- Use explicit labels for saved, delayed, estimated, incomplete, and unavailable data.
- Account details follow the chart range. All accounts shows cash and investment balances, range investment income, and total gain evidence. Cash accounts show range inflows, outflows, net flow and interest, an estimated annualized rate, and average monthly net flow across imported history. Brokerage accounts show range market P/L, current cash, total unrealized and estimated realized P/L, and range dividends and interest.
- Supported SnapTrade account details keep native current cash separate from range and gain metrics.
- Canonical daily history, five-minute intraday projections, latest observations, comparisons, and cash-flow markers remain visually and semantically distinct.
- Chart colors come from shared CSS tokens. Line charts omit persistent series-key legends.
- Shared pie and donut charts place their legends to the right of the visualization.
- Bar-chart legends sit in one centered row below the plot, scrolling horizontally inside the card only when needed.
- Analytics cash flow uses the sources-and-uses Sankey in the primary chart card instead of period bars.
- Cash flow opens first in Analytics by default. Settings controls the order of Analytics charts in the workspace, sidebar, and command menu.
- Spending keeps its range-scoped category bars separate from the companion card containing estimated statement close, daily average, posted and estimated missed credits, and range and all-time largest expenses. It uses the shared chart range control without pointer stepper arrows; Left and Right move between periods from the keyboard. Statement is the default range; month, quarter, year, and all-time ranges use calendar periods. The period label and hero value stay in their own tight stack. Bar segments place credits below zero and expose category, period, amount, and merchant activity on hover or keyboard focus; activity uses actual merchant logos when available and category marks otherwise. Activity sits left of a companion stack with a short Platinum preview above the saved-card Expected activity. Accounts is the canonical cross-account Expected activity location: All accounts shows the combined forecast, while a selected banking account scopes it to that account. Individual brokerage and retirement views omit Expected activity. Banking views place Recent activity left of Expected activity. Compact expected rows reuse the source transaction’s Activity mark, show account and cadence as secondary text, and keep right-aligned estimated amounts with relative near-term due dates. The merchant is a link to Activity with both a description search and the source account filter so the supporting transactions are immediately visible. A keyboard-accessible info icon beside the merchant reveals posted evidence in a tooltip. An unambiguous pending match replaces the due-date label with `Pending` but does not increase the posted observation count. Expected inflows and credits use the positive tone; outflows use the negative tone. Analytics → Amex credits places Current benefit windows below the chart and Overview above Credit activity in the companion column. Activity and sales lists in Analytics, Accounts, Spending, and Holdings stop at the visible window bottom and scroll within their cards. Account Expected activity uses the same viewport-based scroll ceiling as Recent activity. Current benefit windows remain independent of the chart period; estimated Uber Cash stays separate from posted totals. The narrow Spending benefit preview uses a compact cap, progress bar, and single status line; duplicate remaining-period text stays in the full Analytics view.
- Every chart has an accessible label; hover-only information must have a keyboard-accessible equivalent.
- Holdings detail stays scoped to the selected ticker and does not repeat the portfolio-wide holdings table already available on Home. Its price graph is candlestick-only; range remains externally controlled and there is no chart-style toggle. Below the overview, Accounts and Activity share the row with a wider News card whose feed uses the available viewport height.
- Holdings news is newest-first after duplicate grouping. Keep refresh as a compact heading action, show saved stories while disconnected, label relevance and sentiment without provider chrome, and keep rows to source, time, headline, and metrics.
- Never present interpolated or estimated display values as observed financial evidence.

The patched Liveline behavior and upgrade constraints are documented in [../docs/liveline-patch-review.md](../docs/liveline-patch-review.md).

## Interaction and accessibility

- Preserve native button, link, form, and dialog semantics.
- Focus uses a visible two-pixel outline; full-row controls may inset it. The Activity text searches use the transparent focus treatment described above.
- Support keyboard operation for every interactive path.
- The command palette searches Activity descriptions and other ledger fields, previews matching entries, and opens account, category, method, and date filters. Search results use the Activity route's shareable query.
- Keep direct feedback brief and interruptible. Avoid ornamental page transitions.
- Respect `prefers-reduced-motion`; `src/styles.css` reduces animation and transition duration globally.
- Loading, empty, error, hover, active, focus, and disabled states must remain distinguishable.
- Scrollable regions retain keyboard access and visible focus.

Settings → Connections includes an on-demand Plaid recurring check for manual provider evaluation. It uses compact evidence rows for account, category, cadence, observation count, first/last/next dates, provider status, average amount, and last amount. The card states that results are neither saved nor used by Expected activity.

Settings → Design renders synthetic previews of the current tokens and shared components. Its controls are local previews and do not persist financial or display state.
