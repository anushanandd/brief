// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import { COMMAND_HINT_DELAY_MS } from '../components/command-key-hints'
import { FinanceProvider } from '../hooks/finance-provider'
import { LiveMarketProvider } from '../hooks/live-market-provider'
import { ActionProvider } from '../lib/action-system'
import type { ActivityTotals } from '../lib/bindings/ActivityTotals'
import { numberKeyPreference } from '../lib/number-keys'
import { rendererStorage } from '../lib/preference'
import { router } from '../router'

// Keyboard journeys over the mounted app and the browser preview's synthetic seed: effects,
// focus, and the shell's key dispatch all run, which server-rendered route tests can't show.
// Activity's total is here too, since it follows filters and selection as they change.

// Rust's totals by how many rows were asked about; any other set reads what Rust wrote for the
// preview. Selections the seed has no totals for answer from here.
const cannedTotals = vi.hoisted(() => new Map<number, ActivityTotals | Error>())
vi.mock('../lib/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/api')>()
  return {
    ...actual,
    getActivityTotals: async (ids: readonly string[]) => {
      const canned = cannedTotals.get(ids.length)
      if (canned instanceof Error) throw canned
      return canned ?? actual.getActivityTotals(ids)
    },
  }
})

// jsdom has no top layer; opening and closing set the attribute. As in a browser, Escape that
// nothing handled cancels the open modal (below).
function showModal(this: HTMLDialogElement) {
  this.setAttribute('open', '')
}
function close(this: HTMLDialogElement) {
  this.removeAttribute('open')
}

beforeAll(() => {
  // jsdom lays nothing out; list motions skip rows without a box, so every element gets one.
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockReturnValue(
    Object.assign([new DOMRect(0, 0, 10, 10)], { item: () => null }),
  )
  HTMLElement.prototype.scrollIntoView = () => undefined
  // Text measuring falls back when there is no canvas, as it does before fonts load.
  HTMLCanvasElement.prototype.getContext = () => null
  window.matchMedia ??= (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }) satisfies MediaQueryList
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  HTMLDialogElement.prototype.showModal = showModal
  HTMLDialogElement.prototype.close = close
  window.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || event.defaultPrevented) return
    document.querySelector('dialog[open]')?.dispatchEvent(new Event('cancel', { cancelable: true }))
  })
})

// Every test here mounts the whole app; in a full parallel run that outlasts the default five
// seconds.
vi.setConfig({ testTimeout: 30_000 })

// A test that fails before its own unmount would leave its app alive, drawing into the next
// test's page; what is still mounted is taken down here.
const mounted = new Set<() => Promise<void>>()
afterEach(async () => {
  vi.useRealTimers()
  for (const unmount of mounted) await unmount()
  document.body.replaceChildren()
})

// Long enough for an animation frame, which returns focus after a dialog closes.
const settle = () => act(async () => new Promise((resolve) => setTimeout(resolve, 40)))

/** Mounts the app at `path` with its real providers, and waits for `region` to have rows. */
async function mount(path: string, region: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const history = createMemoryHistory({ initialEntries: [path] })
  const testRouter = createRouter({
    routeTree: router.routeTree,
    history,
    context: { queryClient: client },
  })
  await testRouter.load()
  const container = document.body.appendChild(document.createElement('div'))
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <ActionProvider>
          <FinanceProvider>
            <LiveMarketProvider>
              <RouterProvider router={testRouter} />
            </LiveMarketProvider>
          </FinanceProvider>
        </ActionProvider>
      </QueryClientProvider>,
    ),
  )
  const rows = () =>
    [...container.querySelectorAll<HTMLElement>(`[data-keyboard-region]`)]
      .filter((element) => element.getAttribute('aria-label') === region)
      .flatMap((element) => [...element.querySelectorAll<HTMLElement>('[data-keyboard-row]')])
  for (let attempt = 0; attempt < 40 && rows().length < 2; attempt += 1) await settle()
  const unmount = async () => {
    if (!mounted.delete(unmount)) return
    await act(async () => root.unmount())
    client.clear()
  }
  mounted.add(unmount)
  return { history, rows, unmount }
}

async function press(key: string) {
  const target = document.activeElement ?? document.body
  await act(async () => {
    target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
  })
  await settle()
}

/** Enters the list as focus hints or a click would, then steps with J and K. */
async function stepToSecondRow(rows: HTMLElement[]) {
  const [first, second] = rows
  if (!first || !second) throw new Error('The list has two rows')
  await act(async () => first.focus())
  await press('j')
  expect(document.activeElement).toBe(second)
  await press('k')
  expect(document.activeElement).toBe(first)
  await press('j')
  expect(document.activeElement).toBe(second)
  return second
}

it.each([
  ['/activities', 'Activity entries'],
  ['/spending', 'Spending activities'],
])('%s: J and K reach a row, Return opens it, Escape returns to it', async (path, region) => {
  const { rows, unmount } = await mount(path, region)
  const row = await stepToSecondRow(rows())

  await press('Enter')
  const dialog = document.querySelector('dialog[open]')
  expect(dialog).not.toBeNull()
  expect(dialog?.textContent).not.toBe('')

  await press('Escape')
  expect(document.querySelector('dialog[open]')).toBeNull()
  expect(document.activeElement).toBe(row)
  await unmount()
})

it('/: Return on recent activity opens it in Activity, and Escape closes it', async () => {
  const { history, rows, unmount } = await mount('/', 'Activity entries')
  const row = await stepToSecondRow(rows())
  const id = row.dataset.activityId?.replace(/^spending:/, '')

  await press('Enter')
  for (let attempt = 0; attempt < 20 && !document.querySelector('dialog[open]'); attempt += 1)
    await settle()
  expect(history.location.pathname).toBe('/activities')
  expect(new URLSearchParams(history.location.search).get('open')).toBe(id)
  expect(document.querySelector('dialog[open]')).not.toBeNull()

  await press('Escape')
  expect(document.querySelector('dialog[open]')).toBeNull()
  expect(new URLSearchParams(history.location.search).get('open')).toBeNull()
  await unmount()
})

const total = () => document.querySelector('.financial-activity-footer')
const totalFigures = () =>
  [
    ...(total()?.querySelectorAll('.activity-total-figures > .financial-activity-amount') ?? []),
  ].map((amount) => amount.textContent)
/** The Settings picker for a number key, by what it shows. */
const picker = (key: string) =>
  document.querySelector(`[aria-label="What the ${key} key opens"]`)?.textContent
/** Leaves focus on the page, where single keys run. */
const blurFocus = () =>
  act(async () => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
  })
const search = (history: { location: { search: string } }, name: string) =>
  new URLSearchParams(history.location.search).get(name)

const totals = (
  currencies: Array<{ currency: string; net: number }>,
  withoutAmount = 0,
): ActivityTotals => ({
  count: currencies.length + withoutAmount,
  currencies: currencies.map(({ currency, net }) => ({
    currency,
    count: 1,
    moneyIn: Math.max(net, 0),
    moneyOut: Math.min(net, 0),
    net,
  })),
  pending: 0,
  withoutAmount,
  unmatched: 0,
})

describe('/activities total', () => {
  afterEach(() => cannedTotals.clear())

  it('shows one net figure under Amount while narrowed, for the selection when there is one', async () => {
    const { history, rows, unmount } = await mount('/activities', 'Activity entries')
    expect(total()).toBeNull()

    // The seed's three Spotify charges; the figure is Rust's sum, as it wrote it for the preview.
    await act(async () => history.replace('/activities?q=Spotify'))
    for (let attempt = 0; attempt < 20 && !totalFigures().length; attempt += 1) await settle()
    // It is a row of the columned ledger, laid out on the same columns as the rows above it.
    expect(total()?.parentElement?.classList.contains('financial-activity-table')).toBe(true)
    expect(total()?.querySelector('[role="group"]')?.getAttribute('aria-label')).toBe('Total')
    expect(totalFigures()).toEqual(['−$36.97'])
    expect(total()?.textContent).toBe('Total−$36.97')

    // Selecting a row totals the selection instead.
    cannedTotals.set(1, totals([{ currency: 'USD', net: -12.99 }]))
    const [first, second] = rows()
    if (!first || !second) throw new Error('The filtered ledger has rows')
    await act(async () => first.focus())
    await press('x')
    for (let attempt = 0; attempt < 20 && totalFigures()[0] !== '−$12.99'; attempt += 1)
      await settle()
    expect(total()?.querySelector('[role="group"]')?.getAttribute('aria-label')).toBe(
      'Total of the selection',
    )
    expect(totalFigures()).toEqual(['−$12.99'])

    // Two currencies are never added; a row without an amount is counted beside the sum.
    cannedTotals.set(
      2,
      totals(
        [
          { currency: 'USD', net: -5 },
          { currency: 'EUR', net: 3 },
        ],
        1,
      ),
    )
    await act(async () => second.focus())
    await press('x')
    for (let attempt = 0; attempt < 20 && totalFigures().length < 2; attempt += 1) await settle()
    expect(totalFigures()).toEqual(['−$5.00', '€3.00'])
    expect(total()?.textContent).toContain('excludes 1')

    // A sum Rust couldn't give shows no figure, never a zero.
    cannedTotals.set(1, new Error('Rust is unavailable'))
    await act(async () => first.focus())
    await press('x')
    for (let attempt = 0; attempt < 20 && totalFigures().length; attempt += 1) await settle()
    expect(totalFigures()).toEqual([])
    expect(total()?.textContent).toBe('Total')

    await press('Escape')
    await act(async () => history.replace('/activities'))
    await settle()
    expect(total()).toBeNull()
    await unmount()
  })
})

describe('number keys', () => {
  afterEach(() => numberKeyPreference.save({}))

  it('do nothing until assigned, and never show in the sidebar', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    expect(document.querySelector('.sidebar .keys')).toBeNull()
    const shortcuts = [...document.querySelectorAll('.sidebar [aria-keyshortcuts]')].map((link) =>
      link.getAttribute('aria-keyshortcuts'),
    )
    expect(shortcuts.some((keys) => /^\d$/.test(keys ?? ''))).toBe(false)
    await blurFocus()
    for (const key of ['1', '4', '0']) {
      await press(key)
      expect(history.location.pathname).toBe('/activities')
    }
    await unmount()
  })

  it('open the page, report, account, type of account, or security assigned', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await act(async () =>
      numberKeyPreference.save({
        1: { kind: 'page', action: 'navigation.open./calendar', name: 'Calendar' },
        2: { kind: 'page', action: 'navigation.report.fees', name: 'Fees' },
        3: { kind: 'account', account: 'chase', name: 'Checking' },
        // The seed holds one brokerage account, which the type opens.
        4: { kind: 'group', group: 'brokerage', name: 'All brokerage' },
        5: { kind: 'ticker', ticker: 'VTI', name: 'Vanguard Total Stock Market ETF' },
      }),
    )
    await blurFocus()
    await press('1')
    expect(history.location.pathname).toBe('/calendar')
    await press('2')
    expect(history.location.pathname).toBe('/analytics')
    expect(search(history, 'chart')).toBe('fees')
    await press('5')
    expect(history.location.pathname).toBe('/holdings/VTI')
    // Accounts last: jsdom can't run its animated figures (NumberFlow) once they update.
    await press('3')
    expect(history.location.pathname).toBe('/accounts')
    expect(search(history, 'account')).toBe('chase')
    await press('4')
    expect(search(history, 'account')).toBe('fidelity')
    await unmount()
  })

  it('open nothing for a destination that is gone, which Settings names', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await act(async () =>
      numberKeyPreference.save({
        6: { kind: 'ticker', ticker: 'GONE', name: 'Retired Example Fund' },
        7: { kind: 'account', account: 'closed-account', name: 'Old Savings' },
      }),
    )
    await blurFocus()
    await press('6')
    await press('7')
    expect(history.location.pathname).toBe('/activities')

    await act(async () => history.replace('/activities#settings/number-keys'))
    for (let attempt = 0; attempt < 40 && !picker('0'); attempt += 1) await settle()
    expect(picker('6')).toBe('Retired Example Fund · no longer available')
    expect(picker('7')).toBe('Old Savings · no longer available')
    await unmount()
  })

  it('read the first version’s saved list as unassigned', async () => {
    rendererStorage.write(
      numberKeyPreference.key,
      JSON.stringify(['navigation.open./', 'navigation.report.fees', ...Array(8).fill(null)]),
    )
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await blurFocus()
    await press('1')
    await press('2')
    expect(history.location.pathname).toBe('/activities')
    await unmount()
  })

  it('are listed in Settings, one grouped picker per key, and reset to none', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await act(async () =>
      numberKeyPreference.save({
        4: { kind: 'page', action: 'navigation.report.fees', name: 'Fees' },
        5: { kind: 'ticker', ticker: 'VTI', name: 'Vanguard Total Stock Market ETF' },
      }),
    )
    await act(async () => history.replace('/activities#settings/number-keys'))
    for (let attempt = 0; attempt < 40 && !picker('0'); attempt += 1) await settle()
    expect(document.querySelectorAll('#number-keys [aria-label$="key opens"]')).toHaveLength(10)
    expect(picker('1')).toBe('None')
    expect(picker('4')).toBe('Fees')
    expect(picker('5')).toBe('VTI · Vanguard Total Stock Market ETF')

    const reset = [...document.querySelectorAll('#number-keys button')].find(
      (button) => button.textContent === 'Reset',
    )
    if (!(reset instanceof HTMLButtonElement)) throw new Error('Number keys can be reset')
    await act(async () => reset.click())
    expect(picker('4')).toBe('None')
    expect(picker('5')).toBe('None')
    await unmount()
  })

  it('type into a focused field instead of navigating', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await act(async () =>
      numberKeyPreference.save({ 4: { kind: 'page', action: 'navigation.report.fees' } }),
    )
    const field = document.querySelector<HTMLInputElement>('main input')
    if (!field) throw new Error('Activity has a search field')
    await act(async () => field.focus())
    await press('4')
    expect(history.location.pathname).toBe('/activities')
    await unmount()
  })

  it('leave a digit after b to the brokerage jump', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await act(async () => numberKeyPreference.save({ 1: { kind: 'ticker', ticker: 'VTI' } }))
    await blurFocus()
    await press('b')
    await press('1')
    expect(history.location.pathname).toBe('/accounts')
    expect(search(history, 'account')).toBe('fidelity')
    await unmount()
  })
})

/** A key down with ⌘ held; under fake timers nothing runs until they advance. */
const keydown = (key: string) =>
  act(async () => {
    ;(document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent('keydown', { key, metaKey: true, bubbles: true, cancelable: true }),
    )
  })
const keyup = (key: string) =>
  act(async () => {
    document.body.dispatchEvent(
      new KeyboardEvent('keyup', { key, metaKey: false, bubbles: true, cancelable: true }),
    )
  })
const wait = (ms: number) => act(async () => vi.advanceTimersByTime(ms))
const hints = () =>
  [...document.querySelectorAll('.command-key-hint')].map((hint) => hint.textContent)
const holdCommand = async () => {
  await keydown('Meta')
  await wait(COMMAND_HINT_DELAY_MS + 10)
}

describe('⌘ and the sidebar', () => {
  afterEach(() => vi.useRealTimers())

  it('⌘3 opens the third page in the sidebar', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    const third = document.querySelectorAll('.sidebar-menu-button')[2]
    expect(third?.getAttribute('aria-keyshortcuts')).toBe('Meta+3')
    await blurFocus()
    await keydown('3')
    await settle()
    expect(history.location.pathname).toBe(third?.getAttribute('href'))
    await unmount()
  })

  it('holding ⌘ alone shows the shortcuts after a moment, and anything else hides them', async () => {
    const { unmount } = await mount('/activities', 'Activity entries')
    await blurFocus()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })

    await keydown('Meta')
    await wait(COMMAND_HINT_DELAY_MS - 50)
    expect(hints()).toEqual([])
    await wait(60)
    // One per sidebar page, in order, and the rest of the visible controls with ⌘ shortcuts.
    expect(hints()).toEqual(
      expect.arrayContaining(['⌘1', '⌘2', '⌘3', '⌘4', '⌘5', '⌘6', '⌘7', '⌘8', '⌘9', '⌘,']),
    )
    expect(document.querySelector('.command-key-hints')?.getAttribute('aria-hidden')).toBe('true')
    await keyup('Meta')
    expect(hints()).toEqual([])

    await holdCommand()
    expect(hints().length).toBeGreaterThan(0)
    await act(async () => window.dispatchEvent(new Event('blur')))
    expect(hints()).toEqual([])

    await holdCommand()
    await keydown('Shift')
    expect(hints()).toEqual([])

    // A quick ⌘C never shows them, however long ⌘ stays down after.
    await keydown('Meta')
    await wait(100)
    await keydown('c')
    await wait(COMMAND_HINT_DELAY_MS)
    expect(hints()).toEqual([])
    await keyup('Meta')
    await unmount()
  })

  it('⌘K opens the palette without flashing the shortcuts', async () => {
    const { unmount } = await mount('/activities', 'Activity entries')
    await blurFocus()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    await keydown('Meta')
    await wait(100)
    await keydown('k')
    await wait(COMMAND_HINT_DELAY_MS * 2)
    expect(hints()).toEqual([])
    await unmount()
  })

  it('shows nothing while a dialog is open', async () => {
    const { history, unmount } = await mount('/activities', 'Activity entries')
    await act(async () => history.replace('/activities#settings/number-keys'))
    for (let attempt = 0; attempt < 40 && !picker('0'); attempt += 1) await settle()
    expect(document.querySelector('dialog[open]')).not.toBeNull()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    await holdCommand()
    expect(hints()).toEqual([])
    await unmount()
  })
})

// Holdings on Home, and accounts on Accounts, open their own pages.
it.each([
  ['/', 'Holdings'],
  ['/accounts', 'Accounts'],
])('%s: J and K reach a %s row, and Return opens its page', async (path, region) => {
  const { history, rows, unmount } = await mount(path, region)
  const row = await stepToSecondRow(rows())
  const link = row.matches('a') ? row : row.querySelector('[data-keyboard-open]')
  if (!(link instanceof HTMLAnchorElement)) throw new Error('The row links to its page')
  const target = new URL(link.href)

  await press('Enter')
  expect(`${history.location.pathname}${history.location.search}`).toBe(
    `${target.pathname}${target.search}`,
  )
  await unmount()
})
