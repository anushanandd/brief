import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'

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

const sectionViews: Record<SettingsSectionId, () => ReactNode> = {
  connections: ConnectionsSection,
  accounts: AccountsSection,
  taxes: TaxesSection,
  appearance: AppearanceSection,
  charts: ChartsSection,
  navigation: NavigationSection,
  privacy: PrivacySection,
  market: MarketSection,
  data: DataSection,
  advanced: AdvancedSection,
}

/** The section holding a link's target: the section itself, or the one holding the row. */
export function settingsSectionFor(id: string | undefined): SettingsSectionId {
  const section = settingsSections.find(
    (candidate) => candidate.id === id || candidate.entries.some((entry) => entry.id === id),
  )
  return section?.id ?? settingsSections[0].id
}

/**
 * The section being read: the last one whose top has reached the upper part of the scroller,
 * or the last section once the scroller can't go further.
 */
export function sectionInView(
  tops: readonly { id: SettingsSectionId; top: number }[],
  scroller: { scrollTop: number; clientHeight: number; scrollHeight: number },
): SettingsSectionId | undefined {
  if (!tops.length) return undefined
  if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1)
    return tops.at(-1)!.id
  const line = scroller.scrollTop + Math.min(120, scroller.clientHeight / 3)
  return tops.findLast(({ top }) => top <= line)?.id ?? tops[0].id
}

/**
 * Settings' panel: section list and search on the left, and every section in one scroll on the
 * right. The rail jumps to a section and marks the one being read; a search keeps only matches.
 */
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
  const targetId = target ? settingsHashForPath(`/settings#${target}`) : undefined
  const [current, setCurrent] = useState(() => settingsSectionFor(targetId))
  // The first jump lands at once; scrolling down through every section to reach it would only
  // delay what the link asked for.
  const opened = useRef(false)
  // A jump marks its section at once and holds it until the scroll settles, so passing over
  // other sections, or stopping short at the bottom, doesn't move the mark.
  const held = useRef(false)
  const release = useRef(0)
  const holdUntilSettled = useCallback(() => {
    held.current = true
    window.clearTimeout(release.current)
    release.current = window.setTimeout(() => (held.current = false), 150)
  }, [])

  const scrollToTarget = useCallback(
    (id: string) => {
      const element = document.getElementById(id)
      if (!element) return
      const section = element.closest<HTMLElement>('[data-settings-section]')
      if (section) setCurrent(section.id as SettingsSectionId)
      holdUntilSettled()
      const instant = !opened.current || reducedMotion()
      element.scrollIntoView({ block: 'start', behavior: instant ? 'auto' : 'smooth' })
      element.focus({ preventScroll: true })
      if (!element.matches('[data-settings-item]')) return
      element.dataset.flash = ''
      window.setTimeout(() => delete element.dataset.flash, 1600)
    },
    [holdUntilSettled],
  )

  useEffect(() => {
    const id = targetId
    if (!id) {
      // Opening Settings without a destination starts in search, ready to type.
      inputRef.current?.focus({ preventScroll: true })
      opened.current = true
      return undefined
    }
    // A deep link always wins over a search that would hide its target.
    const hiddenBySearch = Boolean(
      searchRef.current && document.getElementById(id)?.closest('[hidden]'),
    )
    if (hiddenBySearch) setQuery('')
    const timer = window.setTimeout(
      () => {
        scrollToTarget(id)
        opened.current = true
      },
      hiddenBySearch ? 60 : 0,
    )
    return () => window.clearTimeout(timer)
  }, [targetId, scrollToTarget])

  // The rail follows the reader: whichever section has reached the top is the current one.
  useEffect(() => {
    const scroller = contentRef.current
    if (!scroller) return undefined
    let frame = 0
    const update = () => {
      frame = 0
      const origin = scroller.getBoundingClientRect().top - scroller.scrollTop
      const tops = [
        ...scroller.querySelectorAll<HTMLElement>('[data-settings-section]:not([hidden])'),
      ].map((section) => ({
        id: section.id as SettingsSectionId,
        top: section.getBoundingClientRect().top - origin,
      }))
      const id = sectionInView(tops, scroller)
      if (id) setCurrent(id)
    }
    const schedule = () => {
      if (held.current) return holdUntilSettled()
      if (!frame) frame = requestAnimationFrame(update)
    }
    scroller.addEventListener('scroll', schedule, { passive: true })
    schedule()
    return () => {
      scroller.removeEventListener('scroll', schedule)
      cancelAnimationFrame(frame)
    }
  }, [search, holdUntilSettled])
  useEffect(() => () => window.clearTimeout(release.current), [])

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
        <header className="settings-panel-header">
          <h2 id={titleId}>Settings</h2>
          <Button
            icon={X}
            size="icon-compact"
            variant="ghost"
            aria-label="Close settings"
            aria-keyshortcuts="Escape"
            onClick={onClose}
          />
        </header>
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
                    aria-current={current === section.id ? 'location' : undefined}
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
          {settingsSections.map(({ id }) => {
            const Section = sectionViews[id]
            return <Section key={id} />
          })}
        </SettingsSearchProvider>
      </div>
    </div>
  )
}
