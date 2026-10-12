# Interface system

Brief should feel like a private macOS financial instrument: calm, compact, and evidence-first. This file records the rules that keep the interface coherent. The tokens in `src/styles/tokens.css` and the shared components in `src/components/` are the implementation; read them for exact values.

## Principles

- **Evidence first.** Show what the data supports. Label saved, delayed, estimated, incomplete, and unavailable values explicitly, and never imply precision with placeholders or color alone.
- **Quiet by default.** Use hierarchy and dense, reusable rows rather than decorative dashboard chrome. Color is for selection, status, category identity, and financial meaning.
- **Say it once.** Headings are short. Put scope, provenance, and instructions beside the value or control they describe, not in a subtitle. Use breadcrumbs for query-backed scope such as account, ticker, or date range.
- **Desktop only.** The app supports widths of 860 px and above. Narrow widths stack cards; there is no mobile navigation model.
- **Cards, not tabs.** Navigation uses the sidebar and cards. Never use horizontal top tabs. Money heads its page and its reports with one strip of cards (Spending, Recurring, then its reports), so moving between them keeps the frame. Investments is headed by its holdings (the portfolio, then each holding by value); its reports open from the period card and lead back through the breadcrumb.
- **One selector.** Money and Investments are each headed by `SelectorStrip`: equal cards, each with its figure and its dollar change over the page's period, opening on that period. Pages never build their own. Accounts have types and grow in number, so Accounts lists them by group instead, and ⌘← and ⌘→ still step through them.
- **One list.** A page shows its transactions once. A breakdown narrows or switches to that list (Spending's categories focus its Activity; a report's Breakdown/Transactions switch, kept in the URL) and never expands into a second copy beside it.
- **Today previews, pages own.** Today is the overview: net worth, what needs you, what is coming, and compact, read-only previews of holdings, accounts, and recent activity, each headed by a link to the page that owns it.
- **One highlight.** Hover uses `--hover-wash` and the current or selected item uses `--sidebar-accent`, in the sidebar, selectors, menus, and the command palette alike. An open report highlights its destination in the sidebar, not its own row.

## Tokens

| Role             | Tokens                                                                                                                                                                    |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Surfaces         | `--background`, `--surface`, `--popup-surface`, `--control-background`, `--border-subtle`                                                                                 |
| Text             | `--text-primary`, `--text-secondary`, `--text-tertiary` (three inks; rules are the ink at `--rule-strength`)                                                              |
| Interaction      | `--accent`, `--accent-soft`, `--control-border`, `--hover-wash`, `--ease-out`, `--ease-in-out`                                                                            |
| Financial status | `--positive`, `--warning`, `--negative`                                                                                                                                   |
| Data categories  | `--data-1` … `--data-7`                                                                                                                                                   |
| Type scale       | `--type-micro`, `--type-meta`, `--type-body`, `--type-section`, `--type-figure` (companion metrics), `--type-page`, `--type-metric`, `--type-hero`                        |
| Geometry         | `--radius-small`, `--radius-inner`, `--radius-control`, `--radius-card`, `--radius-modal`, `--card-padding`, `--card-padding-end`, `--companion-card-width`               |
| Sizes            | `--control-height` (fields), `--button-height`, `--button-height-compact`, `--mark-size`, `--heading-line`, `--strip-item-width`, `--row-height`, `--row-height-compact`  |
| Motion           | `--duration-instant`, `--duration-quick`, `--duration-modal`, `--duration-modal-lift`, `--duration-reveal`, `--ease-out`, `--ease-in-out`, `--ease-lift`, `--ease-reveal` |

Don't add page-local colors, sizes, durations, or easing when a token exists. Spacing stays on the 4 px scale (with 2, 6, 10, and 28 for fine work). `src/styles/tokens.test.ts` enforces spacing, type, radius, motion, and color; a declaration that must leave the scale says why in a `scale-exempt` comment.

## Type

- SF Pro (the system stack) for text, controls, and numbers. Iowan Old Style for the wordmark and page titles.
- Financial values use tabular numerals and come from `src/lib/format.ts`, never a local formatter. Rows, metrics, and selectors show cents. Changes and returns carry two decimals and a sign; shares of a whole (weight, allocation, utilization) carry one decimal and none. Sentences round to whole dollars and one decimal. A change of zero carries no sign, and a negative one carries a true minus (U+2212); copies and exports translate it back to a hyphen. An amount never splits across lines: a metric too wide for its column takes its own row.
- Lists name a date's year only when it isn't the snapshot's year. Chart axes read in the user's time zone; market session marks read in Eastern time and say "ET". Bank descriptors are display-cased (`displayCase`) in titles and kept raw where they are evidence. Hero amounts keep the integer dominant, with the currency symbol and cents smaller and tertiary.
- Icons come from Hugeicons Stroke Rounded through `src/components/icons.tsx`.

## Surfaces and layout

- Cards pad `--card-padding` on every side, or `--card-padding-end` at the bottom when they end in rows. A card's title sits on `--heading-line`; a control beside it overhangs that line instead of making the heading taller, so every title is the same distance from its card's top.
- Cards use the shared surface (`--card-fill`; in Custom, a fade from clear at the top to a light tint at the base), a hairline, the card radius, and standard padding. All borders and dividers are `--hairline`: half a pixel on Retina, a whole one on 1× displays. Nothing draws a focus outline. Popups and tooltips are solid surfaces. Modals (command palette, Settings) lift on `--shadow-modal` over a blurred, dimmed page and rise in on `--modal-enter-from`. Activity details open as an inspector docked to the right edge, over a lighter backdrop, so the list stays readable beside it.
- A primary card sits beside companion cards of `--companion-card-width`. Related cards are 16 px apart, and major groups 24 px.
- `WorkspaceHeader` owns the route title, data status, and route actions.
- The app frame stays fixed. Content scrolls inside the main pane, and long lists scroll inside their cards.
- A card ends where its content ends. Cards side by side align to the top rather than stretching to the taller one, and a full-height list card shrinks to a short list; only a chart's plot keeps a fixed height when it is empty.
- There is no light appearance. Dark follows Robinhood Legend's dark theme: a black canvas with panels set apart by hairlines, grey controls, and blue-grey inks. Custom is the image-backed translucent theme, with a faint grain over its card fade so the gradient can't band. Both share one token contract.
- Scroll bars are hidden everywhere. A card that scrolls shows a cue at its foot (`ScrollCueCard`), and the selector rail fades its edges.
- Icons use four sizes: 12 inline with meta text, 14 inline with body text, 15 inside marks, 16 in controls.

## Color

- Green and red mean favorable and unfavorable only alongside a signed value or text.
- The seven data colors identify categories and series, never gain or loss. Their order and per-theme steps are what keep neighbours apart under colour-vision deficiency. Every theme's series colors reach 3:1 on its surfaces and its tertiary ink reaches 4.5:1, enforced by `src/styles/contrast.test.ts`. A mark still never carries identity without a visible label.
- Unavailable data uses muted text, not a status color. A value that does not apply (earnings for a fund, contributions for an account with no flows) is left out, never dashed. A zero reads $0.00 only when the window holds posted records.
- Personal amounts render through `.sensitive-value` so screen privacy (⌘H) can mask them. Public prices and market charts stay visible.

## Components

Reach for the shared component before creating a variant:

- `Card`, `SectionHeading` (title plus one right-side action), `Metric` (label and value only)
- `Button`: labels stay visible at default and compact sizes. Icon-only buttons need an accessible name. `positive` (repair, connect, save) and `destructive` (forget, remove, erase) are solid fills; everything else stays quiet.
- `ChartRangeSelect` for every compact single choice, including chart periods and Settings options. Activity's date button offers presets only; exact dates are typed in search or set in its filters card.
- Search fields use `AutofillField`: the best match finishes as ghost text and Tab accepts it a word at a time, as in the command palette.
- `EmptyState` for empty, unavailable, or blocked content: one short line, left-aligned, so an empty card never sets the height of its row. Inside a chart it keeps the chart's footprint as a ghost plot so cards don't jump.
- `RankedBars` for largest-first breakdowns (categories, groups), with change against the comparable window when one exists and a tick for the usual amount when history supports one. With `onSelect`, a row selects one item that focuses or filters the page's list; a row expands to the activity behind it only where no list beside it shows that activity.
- Shared account, activity, position, and sale rows for financial lists.
- When the Activity ledger is too narrow, places, then categories, then accounts give way; amounts and dates never clip. Missing merchant logos fall back to the transaction or category mark, never generated initials.
- Tooltips hold compact evidence for chart and data marks only. Interactive or explanatory content belongs in a click-to-open popover.

## Charts and data

- Every chart has an accessible label.
- Investments follows the app's chart range, chosen in its main chart card, and every card there follows it. No other card carries its own range.
- News carries each story's relevance to its company (0–100) as a quiet badge; stories that only mention the company fold away. Summaries are made in the background, never behind a button.
- The investment summary is a short paragraph about today (the move against the S&P 500 and who led it), then up to two headlines with their publisher, time, and link. It repeats nothing another card shows and never phrases a cause itself.
- Keep canonical history, intraday projections, latest observations, comparisons, and cash-flow markers visually distinct.
- Never present interpolated or estimated values as observed evidence. A range change with no comparison reads "Change unavailable".
- Chart colors come from tokens. Line charts have no persistent legend; a comparison chart may carry a caption that states the compared values.
- The patched chart library is documented in [docs/liveline-patch-review.md](../docs/liveline-patch-review.md).

## Interaction and accessibility

- Brief behaves as a Mac app. Every app and page command appears in the menu bar, grouped Go, View, and Actions; only Command shortcuts that never edit text get a menu key equivalent. Rows have a context menu (Open, Copy Row, Copy Amount) and ⌘C copies the focused row; both leave out what screen privacy hides. `brief://` links open any view (`brief://holdings/VTI`), and the context menu copies one for the current view.
- Every control outside a list row names its command in `data-command`, so the pointer reaches nothing the palette, menu bar, and keyboard can't. Rows are covered by the list commands. `src/pages/routes.test.tsx` checks this on every page.
- Keep native button, link, form, and dialog semantics. Stateful controls expose `aria-pressed`, `aria-expanded`, `aria-busy`, or disabled state.
- The keyboard works through shortcuts, not focus: no element draws a focus ring and Tab doesn't move focus. Text fields still take focus for typing. Menus and dialogs hand focus back to their opener so shortcuts like the arrow keys continue in place.
- Arrows have one meaning everywhere: ⌘↑ and ⌘↓ move between sidebar pages, ⌘← and ⌘→ step through the page's selector, bare ← and → move through time where a page has periods, and bare ↑ and ↓ move through the focused list like J and K (on Activity, ↓ selects the first row and F opens the filters card beside the ledger). Command arrows stay with a text field while it has focus.
- Shortcuts are defined once in the action registry and drive the command palette (⌘K), the key guide (`?`), and Keyboard shortcuts (⌘/). Bare shortcuts pause during text entry. `g` then a letter opens a destination and `g r` then a letter opens a report. App shortcuts pause while a dialog is open, so the transaction inspector handles its own keys (J/K, C, T, S, F) and lists them in its footer.
- Motion is brief and interruptible. Animate transform, opacity, or color with `--ease-out`, never layout properties. Frequent keyboard actions don't animate. `prefers-reduced-motion` reduces all motion.
- The page arrives once per launch, after the window shows: its blocks rise 12 px and come into focus from a 3 px blur, in reading order. Nothing else uses blur.
- Speed is a feature: a step through the sidebar or a selector paints within 100 ms (`navigationBudgetMs`), and a selector loads the cards either side ahead of the step.
- Loading, empty, error, hover, active, and disabled states must stay distinguishable.
- Consequential actions (recovery, erasure, credential changes) use an in-app confirmation with Cancel focused, pending feedback, and inline errors.

## Settings

Settings is a modal dialog, not a route. `#settings/<section-or-row>` opens it at a target, and ⌘, opens it. In-app links use `SettingsLink`. Rows sit directly on the dialog surface, separated by hairlines: title and consequence on the left, control on the right. A row has a description only when its title isn't self-explanatory, and a billed option's description states the cost. Each section that owns preferences has its own Reset. Settings stays usable when the snapshot can't load.
