export type SettingsSectionId =
  | 'connections'
  | 'accounts'
  | 'taxes'
  | 'appearance'
  | 'charts'
  | 'navigation'
  | 'privacy'
  | 'market'
  | 'data'
  | 'advanced'

export type SettingsEntry = {
  id: string
  title: string
  keywords: string
}

export type SettingsSection = {
  id: SettingsSectionId
  title: string
  keywords: string
  entries: readonly SettingsEntry[]
}

// One index drives filtering, the section rail, and deep links. Each entry id is
// rendered as the element id of its row, so links like #platinum-benefits work.
export const settingsSections: readonly SettingsSection[] = [
  {
    id: 'connections',
    title: 'Connections',
    keywords: 'providers integrations data sources network',
    entries: [
      {
        id: 'connection-plaid',
        title: 'Plaid',
        keywords:
          'bank banking credit card brokerage stock plan institutions credentials client id secret keychain connect repair forget',
      },
      {
        id: 'connection-snaptrade',
        title: 'SnapTrade',
        keywords:
          'investments brokerage positions balances credentials client id consumer key keychain connect',
      },
      {
        id: 'connection-alpaca',
        title: 'Alpaca',
        keywords:
          'market quotes live prices benchmark price history credentials api key secret keychain',
      },
      {
        id: 'connection-alphavantage',
        title: 'Alpha Vantage',
        keywords: 'earnings dates credentials api key keychain',
      },
      {
        id: 'connection-sec',
        title: 'SEC EDGAR',
        keywords: 'filings 8-k 6-k sec edgar contact email news events sector industry',
      },
      {
        id: 'plaid-live-balances',
        title: 'Live bank balances',
        keywords: 'plaid real-time balance refresh now billed cost',
      },
      {
        id: 'plaid-statement-details',
        title: 'Card statement details',
        keywords: 'plaid liabilities statement due date minimum payment apr credit card billed',
      },
      {
        id: 'plaid-investment-activity',
        title: 'Investment activity',
        keywords:
          'plaid investments transactions trades dividends contributions 401k ira retirement billed',
      },
      {
        id: 'plaid-recurring-streams',
        title: 'Recurring streams',
        keywords: 'plaid recurring transactions subscriptions bills streams cancelled billed',
      },
    ],
  },
  {
    id: 'accounts',
    title: 'Accounts',
    keywords: 'accounts spending',
    entries: [
      {
        id: 'spending-account',
        title: 'Spending account',
        keywords: 'credit card amex statement spending page',
      },
      {
        id: 'card-benefits',
        title: 'Other card benefits',
        keywords: 'second card product credits track additional',
      },
      {
        id: 'platinum-benefits',
        title: 'Platinum benefits',
        keywords: 'amex credits uber resy hotel airline clear visible hide',
      },
      {
        id: 'manual-accounts',
        title: 'Manual accounts',
        keywords: 'add account by hand 401k property car loan balance net worth',
      },
      {
        id: 'account-details',
        title: 'Names, start dates, and closed accounts',
        keywords: 'display name rename nickname start date history begin',
      },
      {
        id: 'statement-history',
        title: 'Statement history',
        keywords:
          'import statement csv xlsx excel export older history transactions activity upload',
      },
      {
        id: 'linked-accounts',
        title: 'Linked accounts',
        keywords: 'duplicate merge link unlink plaid snaptrade same account',
      },
    ],
  },
  {
    id: 'taxes',
    title: 'Taxes',
    keywords: 'tax estimate capital gains dividends interest wash sale harvest 1099',
    entries: [
      {
        id: 'tax-filing-status',
        title: 'Filing status',
        keywords: 'single married jointly separately head of household profile',
      },
      {
        id: 'tax-income',
        title: 'Other taxable income',
        keywords: 'wages salary income bracket form 1040 marginal rate',
      },
      { id: 'tax-state-rate', title: 'State income tax', keywords: 'state rate percent' },
      {
        id: 'account-tax-treatment',
        title: 'Account tax treatment',
        keywords: 'taxable tax-deferred tax-free roth ira hsa 529 retirement 401k pre-tax',
      },
      {
        id: 'retirement-birth-year',
        title: 'Birth year',
        keywords: 'age catch-up retirement limit 50',
      },
      {
        id: 'retirement-planned-costs',
        title: 'Costs you’ll add',
        keywords: 'rent housing health insurance move out budget fire retirement',
      },
      {
        id: 'retirement-earlier-contributions',
        title: 'Earlier Roth contributions',
        keywords: 'roth ira basis contributions 5498 before history',
      },
    ],
  },
  {
    id: 'appearance',
    title: 'Appearance',
    keywords: 'look display',
    entries: [
      {
        id: 'theme',
        title: 'Theme',
        keywords: 'light dark system brief translucent glass mode color',
      },
      {
        id: 'home-accounts',
        title: 'Account categories',
        keywords: 'home today categories groups expanded collapsed',
      },
    ],
  },
  {
    id: 'charts',
    title: 'Charts',
    keywords: 'graphs defaults range',
    entries: [
      {
        id: 'home-range',
        title: 'Chart range',
        keywords: 'home overview accounts investments holdings default chart period week month',
      },
    ],
  },
  {
    id: 'navigation',
    title: 'Navigation',
    keywords: 'order sidebar',
    entries: [
      { id: 'page-order', title: 'Page order', keywords: 'sidebar pages reorder shortcuts' },
    ],
  },
  {
    id: 'privacy',
    title: 'Privacy',
    keywords: 'security masking',
    entries: [
      {
        id: 'external-logos',
        title: 'Logos',
        keywords: 'logo.dev network images icons company merchant bank',
      },
      {
        id: 'hide-on-launch',
        title: 'Hide values when Brief opens',
        keywords: 'mask blur sensitive launch startup',
      },
      {
        id: 'hide-on-blur',
        title: 'Hide values when the window loses focus',
        keywords: 'mask blur sensitive background inactive',
      },
      {
        id: 'apple-intelligence',
        title: 'Apple Intelligence',
        keywords: 'ai on-device model explanations',
      },
    ],
  },
  {
    id: 'market',
    title: 'Market data',
    keywords: 'prices quotes',
    entries: [
      {
        id: 'live-price-updates',
        title: 'Live price updates',
        keywords: 'frequency interval refresh stream realtime',
      },
    ],
  },
  {
    id: 'data',
    title: 'Backup & data',
    keywords: 'storage files',
    entries: [
      {
        id: 'financial-backup',
        title: 'Financial backup',
        keywords: 'export save restore import briefbackup',
      },
      {
        id: 'preferences-file',
        title: 'Preferences file',
        keywords: 'export import settings briefsettings transfer',
      },
      {
        id: 'exports',
        title: 'Exports',
        keywords: 'calendar ics bills dividends statement dates lots csv tax accountant',
      },
      {
        id: 'local-storage',
        title: 'Local storage',
        keywords: 'database keychain caches where stored disk',
      },
      {
        id: 'erase-data',
        title: 'Erase local financial data',
        keywords: 'delete wipe reset remove clear',
      },
    ],
  },
  {
    id: 'advanced',
    title: 'Advanced',
    keywords: 'developer',
    entries: [
      {
        id: 'background-refresh',
        title: 'Background refresh',
        keywords: 'daily refresh closed window keep running history gaps',
      },
      {
        id: 'notifications',
        title: 'Notifications',
        keywords: 'alerts payment due credit expiring cash shortfall menu bar',
      },
      {
        id: 'brief-calendar',
        title: 'Brief calendar',
        keywords: 'calendar eventkit events sync icloud due dates trips',
      },
      {
        id: 'open-at-login',
        title: 'Open at login',
        keywords: 'login items startup launch restart',
      },
      {
        id: 'global-shortcut',
        title: 'System-wide shortcut',
        keywords: 'hotkey palette anywhere summon command b',
      },
      {
        id: 'diagnostics',
        title: 'Diagnostics',
        keywords: 'data health logs sync history design reference troubleshooting',
      },
      { id: 'about', title: 'About Brief', keywords: 'version backend app' },
    ],
  },
]

const normalize = (value: string) =>
  value
    .toLocaleLowerCase()
    .normalize('NFKD')
    .replaceAll(/[̀-ͯ]/g, '')

const tokens = (query: string) => normalize(query).split(/\s+/).filter(Boolean)

export type SettingsSearchResult = {
  query: string
  entryIds: ReadonlySet<string>
  sectionIds: ReadonlySet<SettingsSectionId>
  counts: ReadonlyMap<SettingsSectionId, number>
  total: number
}

/** Every query word must appear in the entry's title, keywords, or its section. */
export function searchSettings(query: string): SettingsSearchResult | null {
  const words = tokens(query)
  if (!words.length) return null
  const entryIds = new Set<string>()
  const sectionIds = new Set<SettingsSectionId>()
  const counts = new Map<SettingsSectionId, number>()
  for (const section of settingsSections) {
    const sectionText = normalize(`${section.title} ${section.keywords}`)
    for (const entry of section.entries) {
      const text = `${sectionText} ${normalize(`${entry.title} ${entry.keywords}`)}`
      if (!words.every((word) => text.includes(word))) continue
      entryIds.add(entry.id)
      sectionIds.add(section.id)
      counts.set(section.id, (counts.get(section.id) ?? 0) + 1)
    }
  }
  return { query, entryIds, sectionIds, counts, total: entryIds.size }
}

/** Ranges of `text` that match any query word, for highlighting titles. */
export function settingsMatchRanges(text: string, query: string) {
  const haystack = normalize(text)
  const ranges: Array<[number, number]> = []
  for (const word of tokens(query)) {
    let index = haystack.indexOf(word)
    while (index >= 0) {
      ranges.push([index, index + word.length])
      index = haystack.indexOf(word, index + word.length)
    }
  }
  ranges.sort((left, right) => left[0] - right[0])
  return ranges.reduce<Array<[number, number]>>((merged, range) => {
    const last = merged.at(-1)
    if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1])
    else merged.push([...range])
    return merged
  }, [])
}

const legacySettingsPaths: Record<string, SettingsSectionId> = {
  '/settings/appearance': 'appearance',
  '/settings/accounts': 'accounts',
  '/settings/connections': 'connections',
  '/settings/data': 'privacy',
  '/settings/advanced': 'advanced',
}

const legacyHashes: Record<string, string> = { 'data-sources': 'connections' }

/** Maps former Settings detail routes (still emitted by native health checks) to anchors. */
export function settingsHashForPath(path: string) {
  const [pathname, hash] = path.split('#')
  if (hash) return legacyHashes[hash] ?? hash
  return legacySettingsPaths[pathname]
}
