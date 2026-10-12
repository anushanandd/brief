import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { RefObject } from 'react'

import { acceptAutofill, AutofillField, autofillCompletion } from '../components/autofill'
import {
  ChartCandlestick,
  ChartNoAxesCombined,
  ChevronRight,
  Database,
  EyeOff,
  Landmark,
  Link2,
  List,
  Palette,
  ReceiptText,
  Search,
  WandSparkles,
  Wrench,
  X,
} from '../components/icons'
import type { IconComponent } from '../components/icons'
import { SettingsSearchProvider } from '../components/settings-ui'
import { AccountsSection } from '../components/settings/accounts'
import { AdvancedSection, MarketSection } from '../components/settings/advanced'
import {
  AppearanceSection,
  ChartsSection,
  NavigationSection,
} from '../components/settings/appearance'
import { ConnectionsSection } from '../components/settings/connections'
import { DataSection, PrivacySection } from '../components/settings/data'
import { RulesSection } from '../components/settings/rules'
import { TaxesSection } from '../components/settings/taxes'
import { Button, EmptyState, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { getDataHealth } from '../lib/api'
import { countOf, formatUpdatedAt } from '../lib/format'
import {
  searchSettings,
  settingsHashForPath,
  settingsSections,
  type SettingsSectionId,
} from '../lib/settings-search'

function reducedMotion() {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
}

const sectionIcons: Record<SettingsSectionId, IconComponent> = {
  connections: Link2,
  rules: WandSparkles,
  accounts: Landmark,
  taxes: ReceiptText,
  appearance: Palette,
  charts: ChartNoAxesCombined,
  navigation: List,
  privacy: EyeOff,
  market: ChartCandlestick,
  data: Database,
  advanced: Wrench,
}

function TrustSummary({ onShow }: { onShow: (id: string) => void }) {
  const finance = useFinance()
  const health = useQuery({
    queryKey: ['data-health', finance.data?.revision],
    queryFn: getDataHealth,
    enabled: Boolean(finance.data),
  })
  const providers = Object.values(finance.integrationStatus)
  const configured = providers.filter(Boolean).length
  const healthCount = health.data?.issues.length
  return (
    <div className="settings-trust-card" role="group" aria-label="Status summary">
      <div className="settings-trust-item">
        <StatusDot tone={finance.data ? 'positive' : finance.isError ? 'negative' : 'neutral'} />
        <span>
          <small>Last saved</small>
          <strong>
            {finance.data
              ? formatUpdatedAt(finance.data.updatedAt)
              : finance.isError
                ? 'Unavailable'
                : 'Loading…'}
          </strong>
        </span>
      </div>
      <a
        href="#settings/connections"
        className="settings-trust-item settings-trust-link"
        onClick={(event) => {
          event.preventDefault()
          onShow('connections')
        }}
      >
        <Link2 size={16} aria-hidden="true" />
        <span>
          <small>Connections</small>
          <strong>
            {configured ? `${configured} of ${providers.length} configured` : 'None configured'}
          </strong>
        </span>
        <ChevronRight size={15} aria-hidden="true" />
      </a>
      <Link to="/health" className="settings-trust-item settings-trust-link">
        <StatusDot
          tone={
            health.isError || health.data?.overallStatus === 'critical'
              ? 'negative'
              : health.data?.overallStatus === 'healthy'
                ? 'positive'
                : 'warning'
          }
        />
        <span>
          <small>Data health</small>
          <strong>
            {health.isPending
              ? 'Checking…'
              : health.isError
                ? 'Unavailable'
                : healthCount
                  ? countOf(healthCount, 'issue')
                  : 'All checks passed'}
          </strong>
        </span>
        <ChevronRight size={15} aria-hidden="true" />
      </Link>
    </div>
  )
}

/** Tracks the section nearest the top of the scrolling pane for the section list. */
function useActiveSection(content: RefObject<HTMLElement | null>, dependency: unknown) {
  const [active, setActive] = useState<string>(settingsSections[0].id)
  useEffect(() => {
    const root = content.current
    const sections = [
      ...(root?.querySelectorAll<HTMLElement>('[data-settings-section]') ?? []),
    ].filter((section) => !section.hidden)
    if (!root || !sections.length || typeof IntersectionObserver === 'undefined') return undefined
    const visible = new Map<string, boolean>()
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) visible.set(entry.target.id, entry.isIntersecting)
        const first = sections.find((section) => visible.get(section.id))
        if (first) setActive(first.id)
      },
      { root, rootMargin: '-24px 0px -60% 0px' },
    )
    for (const section of sections) observer.observe(section)
    return () => observer.disconnect()
  }, [content, dependency])
  return [active, setActive] as const
}

/** The Settings dialog body: section list and search on the left, every section on the right. */
export function SettingsPanel({
  target,
  titleId,
  onShow,
  onClose,
}: {
  target: string
  titleId: string
  onShow: (id?: string) => void
  onClose: () => void
}) {
  const finance = useFinance()
  const [query, setQuery] = useState('')
  const search = useMemo(() => searchSettings(query), [query])
  const searchRef = useRef(search)
  searchRef.current = search
  const inputRef = useRef<HTMLInputElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const [active, setActive] = useActiveSection(contentRef, search)

  const scrollToTarget = useCallback(
    (id: string) => {
      const element = document.getElementById(id)
      if (!element) return
      element.scrollIntoView({ block: 'start', behavior: reducedMotion() ? 'auto' : 'smooth' })
      element.focus({ preventScroll: true })
      const section = element.closest<HTMLElement>('[data-settings-section]')
      if (section) setActive(section.id)
      if (!element.matches('[data-settings-item]')) return
      element.dataset.flash = ''
      window.setTimeout(() => delete element.dataset.flash, 1600)
    },
    [setActive],
  )

  useEffect(() => {
    const id = target ? settingsHashForPath(`/settings#${target}`) : undefined
    if (!id) {
      // Opening Settings without a destination starts in search, ready to type.
      inputRef.current?.focus({ preventScroll: true })
      return undefined
    }
    // A deep link always wins over a search that would hide its target.
    const hiddenBySearch = Boolean(
      searchRef.current && document.getElementById(id)?.closest('[hidden]'),
    )
    if (hiddenBySearch) setQuery('')
    const timer = window.setTimeout(() => scrollToTarget(id), hiddenBySearch ? 60 : 0)
    return () => window.clearTimeout(timer)
  }, [target, scrollToTarget])

  const firstResult = search
    ? settingsSections.flatMap(({ entries }) => entries).find(({ id }) => search.entryIds.has(id))
    : undefined

  const completion = autofillCompletion(query, [
    ...settingsSections.map(({ title }) => title),
    ...settingsSections.flatMap(({ entries }) => entries.map(({ title }) => title)),
  ])

  return (
    <div
      className="settings-panel"
      onKeyDown={(event) => {
        if (event.key === '/' && event.target !== inputRef.current) {
          event.preventDefault()
          inputRef.current?.focus()
        }
      }}
    >
      <aside className="settings-panel-nav">
        <h2 id={titleId}>Settings</h2>
        <div className="settings-search">
          <Search size={15} aria-hidden="true" />
          <AutofillField query={query} completion={completion}>
            <input
              ref={inputRef}
              type="text"
              aria-label="Search settings"
              aria-describedby="settings-search-status"
              placeholder="Search"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (acceptAutofill(event, completion, setQuery)) return
                if (event.key !== 'Enter' || !firstResult) return
                event.preventDefault()
                scrollToTarget(firstResult.id)
              }}
            />
          </AutofillField>
          {query ? (
            <Button
              size="icon-compact"
              variant="ghost"
              aria-label="Clear search"
              onClick={() => {
                setQuery('')
                inputRef.current?.focus()
              }}
            >
              <X size={12} aria-hidden="true" />
            </Button>
          ) : (
            <kbd aria-hidden="true">/</kbd>
          )}
        </div>
        <p id="settings-search-status" className="settings-search-status" role="status">
          {search?.total ? `${countOf(search.total, 'match', 'matches')} · Return to jump` : ''}
        </p>
        <nav aria-label="Settings sections">
          <ul>
            {settingsSections.map((section) => {
              const Icon = sectionIcons[section.id]
              const count = search?.counts.get(section.id) ?? 0
              const dimmed = Boolean(search && !count)
              return (
                <li key={section.id}>
                  <a
                    href={`#settings/${section.id}`}
                    className="settings-rail-link"
                    aria-current={!search && active === section.id ? 'location' : undefined}
                    aria-disabled={dimmed || undefined}
                    tabIndex={dimmed ? -1 : undefined}
                    onClick={(event) => {
                      event.preventDefault()
                      if (dimmed) return
                      if (target === section.id) scrollToTarget(section.id)
                      else onShow(section.id)
                    }}
                  >
                    <Icon size={16} aria-hidden="true" />
                    <span>{section.title}</span>
                    {search && count ? <small>{count}</small> : null}
                  </a>
                </li>
              )
            })}
          </ul>
        </nav>
      </aside>

      <div className="settings-panel-content" ref={contentRef}>
        <Button
          icon={X}
          size="icon"
          variant="ghost"
          className="settings-close"
          aria-label="Close settings"
          aria-keyshortcuts="Escape"
          onClick={onClose}
        />
        {finance.annotationWarning ? (
          <p className="settings-overview-warning" role="status">
            {finance.annotationWarning}
          </p>
        ) : null}
        <SettingsSearchProvider value={search}>
          {search ? null : <TrustSummary onShow={onShow} />}
          {search && !search.total ? (
            <EmptyState title={`No settings match “${query.trim()}”`}>
              Try theme, backup, or credentials.
            </EmptyState>
          ) : null}
          <ConnectionsSection />
          <AccountsSection />
          <TaxesSection />
          <RulesSection />
          <AppearanceSection />
          <ChartsSection />
          <NavigationSection />
          <PrivacySection />
          <MarketSection />
          <DataSection />
          <AdvancedSection />
        </SettingsSearchProvider>
      </div>
    </div>
  )
}
