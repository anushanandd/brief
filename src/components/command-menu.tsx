import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { Command } from 'cmdk'
import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react'

import { useAskBrief } from '../hooks/use-ask-brief'
import { useFinance } from '../hooks/use-finance'
import { useFinanceRefreshState, useRefreshFinance } from '../hooks/use-refresh-finance'
import { useRunVerb } from '../hooks/use-run-verb'
import { accountDisplayName } from '../lib/account-names'
import {
  forgetActionUsage,
  getActionUsage,
  recordActionUsage,
  useRegisteredActions,
  type AppAction,
} from '../lib/action-system'
import { activityMatchesSearch, buildActivities, linkedAccountIds } from '../lib/activity'
import { financeSettings } from '../lib/finance-settings'
import { countOf } from '../lib/format'
import { ledgerView } from '../lib/ledger-view'
import { getExternalLogosEnabled } from '../lib/logos'
import { useOrderedNavigation } from '../lib/navigation-preferences'
import { objectKinds, objectQuery, searchObjects, type ObjectKind } from '../lib/objects'
import { activityMatchesChips, activityTerm, filterFields } from '../lib/palette-filters'
import {
  forgetQueryPicks,
  getPaletteFavorites,
  getQueryUsage,
  moveFavorite,
  paletteMode,
  paletteTabs,
  recordQueryPick,
  savePaletteFavorites,
  toggleFavorite,
  type PaletteMode,
} from '../lib/palette-ranking'
import {
  menuRows,
  paletteSections,
  type MenuAction,
  type PaletteSection,
  type RowAction,
} from '../lib/palette-rows'
import { initialPalette, paletteReducer } from '../lib/palette-state'
import { isPinned, savePins, usePins, withPin } from '../lib/pins'
import { verbById } from '../lib/verbs'
import { acceptAutofill, AutofillField, autofillCompletion } from './autofill'
import { ActionsPopover, useActionsMenu } from './command-menu-popover'
import {
  activityRow,
  choiceRow,
  filterRows,
  FooterHint,
  Highlighted,
  paletteActions,
  RowMark,
  rowObjectId,
} from './command-menu-rows'
import { Ellipsis, RefreshCw, Star, X } from './icons'
import { Keys } from './keys'
import { LedgerResult } from './ledger-result'
import { useShowObject } from './object-details'

const plainMeta = (event: KeyboardEvent, key: string) =>
  event.key.toLowerCase() === key && event.metaKey && !event.ctrlKey && !event.shiftKey

// Accounts, holdings, and Activity have rows of their own outside the Objects tab, which
// read the snapshot's richer fields; Activity's rows come from the search below.
const ownRows = new Set<ObjectKind>(['account', 'holding', 'transaction', 'trade'])
/** How many Activity entries one search reads; the count shows `+` when it reaches it. */
const ACTIVITY_LIMIT = 100
// `account: chase`: a field-scoped Activity filter, which the palette applies itself.
const scopedFilter = /^(description|account|category|tag|person|method|location|amount|date):/i

/**
 * Rust's search for what the palette shows: words, or, with kinds and no words, every object
 * of those kinds, as a verb's question lists them.
 */
function useObjectHits(
  lookup: { text: string; kinds: ObjectKind[] },
  limit: number,
  enabled: boolean,
) {
  const text = lookup.text.trim()
  const asks = enabled && (text.length >= 2 || (lookup.kinds.length > 0 && !text))
  const search = useQuery({
    queryKey: ['object-search', text, lookup.kinds, limit],
    queryFn: () => searchObjects({ text, kinds: lookup.kinds, limit }),
    enabled: asks,
    placeholderData: keepPreviousData,
    staleTime: 30_000,
    retry: false,
  })
  return asks ? (search.data ?? []) : []
}

/**
 * The command palette: one search over actions, pages, accounts, holdings, and activity, with
 * type tabs, a `/` filter picker, an Actions menu beside each row, and an Ask mode for Apple
 * Intelligence. Rows come from `command-menu-rows`; their order from `palette-rows`.
 */
export function CommandMenu({
  open,
  onOpenChange,
  onShowShortcuts,
  onRunAction,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onShowShortcuts: () => void
  onRunAction?: (action: AppAction) => void
}) {
  const pages = useOrderedNavigation()
  const navigate = useNavigate()
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const isRefreshing = useFinanceRefreshState()
  const registeredActions = useRegisteredActions()
  const dialog = useRef<HTMLDialogElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const list = useRef<HTMLDivElement>(null)
  const [palette, dispatch] = useReducer(paletteReducer, paletteMode.get(), initialPalette)
  const { mode, query, tabId, chips, filterStep, verbStep, selected } = palette
  const askingVerb = verbStep ? verbById(verbStep) : undefined
  const [favorites, setFavorites] = useState(getPaletteFavorites)
  const [notice, setNotice] = useState<string>()
  const [, setRankingVersion] = useState(0)
  const pins = usePins()
  const trimmedQuery = query.trim()
  const askMode = mode === 'ask'
  const tab = paletteTabs.find(({ id }) => id === tabId) ?? paletteTabs[0]
  const ask = useAskBrief(askMode, Boolean(finance.data))
  const askReady = ask.ready(trimmedQuery)
  const names = financeSettings(finance.data).accountNames
  const accountName = (id: string, name: string) => accountDisplayName(id, name, names)
  const [externalLogos] = useState(getExternalLogosEnabled)
  const referenceIso = finance.data?.updatedAt ?? ''
  const activities = useMemo(
    () =>
      finance.data
        ? buildActivities(finance.data, financeSettings(finance.data).accountNames, externalLogos)
        : [],
    [externalLogos, finance.data],
  )
  const activityAccountIds = linkedAccountIds(
    activities.map(({ accountId }) => accountId),
    finance.data?.accountLinks,
  )
  const textSearch = activityTerm(trimmedQuery).length >= 2
  const searching = !askMode && !filterStep && !askingVerb
  const searchesActivity = searching && (textSearch || chips.length > 0)
  const showObject = useShowObject()
  const runVerb = useRunVerb()
  const objectsTab = tab.id === 'objects'
  // A verb's question searches its kind; the Objects tab may name one; elsewhere, every kind.
  const objectHits = useObjectHits(
    askingVerb?.object
      ? { text: trimmedQuery, kinds: [askingVerb.object] }
      : objectsTab
        ? objectQuery(trimmedQuery)
        : { text: trimmedQuery, kinds: [] },
    40,
    (searching && !chips.length) || Boolean(askingVerb),
  )
  // Free text finds Activity through Rust's search; chips and field-scoped filters narrow
  // the entries here, as Activity's own filters do.
  const freeText = textSearch && !scopedFilter.test(trimmedQuery)
  const transactionHits = useObjectHits(
    { text: trimmedQuery, kinds: ['transaction', 'trade'] },
    ACTIVITY_LIMIT,
    searchesActivity && freeText,
  )
  const activityById = useMemo(
    () => new Map(activities.map((activity) => [activity.id, activity])),
    [activities],
  )
  const activityMatches = !searchesActivity
    ? []
    : freeText
      ? transactionHits.flatMap((hit) => {
          const id =
            hit.kind === 'trade' ? hit.id : `spending:${hit.id.slice('transaction:'.length)}`
          const activity = activityById.get(id)
          return activity && activityMatchesChips(activity, chips) ? [activity] : []
        })
      : activities.filter(
          (activity) =>
            activityMatchesChips(activity, chips) &&
            (!textSearch || activityMatchesSearch(activity, trimmedQuery)),
        )

  const rowContext = {
    navigate,
    referenceIso,
    query: trimmedQuery,
    externalLogos,
    names,
    accountName,
  }
  const actions = paletteActions({
    ...rowContext,
    finance: finance.data,
    registeredActions,
    pages,
    chips,
    searchesActivity,
    textSearch,
    activityMatches,
    activityAccountIds,
    isRefreshing,
    refresh,
    onShowShortcuts,
    activityCapped: freeText && transactionHits.length >= ACTIVITY_LIMIT,
    objectHits: objectsTab ? objectHits : objectHits.filter((hit) => !ownRows.has(hit.kind)),
    chooseVerb: (verb) => {
      dispatch({ type: 'chooseVerb', verb: verb.id })
      input.current?.focus()
    },
    showObject: (id) => {
      onOpenChange(false)
      showObject(id)
    },
    runVerb: (verb, objectId) => {
      onOpenChange(false)
      void runVerb(verb, objectId)
    },
  })

  const pickerRows = filterRows(filterStep, {
    finance: finance.data,
    activities,
    activityAccountIds,
    accountName,
    chooseField: (field) => dispatch({ type: 'chooseField', field }),
    addChip: (chip) => dispatch({ type: 'addChip', chip }),
  })

  // Read on every render; "Reset ranking" bumps a counter to force one.
  const favoriteIds = new Set(favorites)
  const ranking = {
    query: trimmedQuery,
    usage: getActionUsage(),
    queryUsage: getQueryUsage(),
    favorites: favoriteIds,
    now: Date.now(),
  }
  // A verb's question lists only the objects it could act on, in search's order.
  const sections: PaletteSection[] = askingVerb?.object
    ? [
        {
          heading: objectKinds[askingVerb.object].plural,
          items: objectHits.map((hit, index) =>
            choiceRow(hit, index, askingVerb, (verb, objectId) => {
              onOpenChange(false)
              void runVerb(verb, objectId)
            }),
          ),
        },
      ]
    : paletteSections({
        askMode,
        filterStep,
        filterRows: pickerRows,
        tab,
        chipCount: chips.length,
        actions,
        favorites,
        recentActivity: activities.slice(0, 8).map((activity) => activityRow(activity, rowContext)),
        ranking,
      })
  const rows = sections.flatMap(({ items }) => items)
  const firstEnabled = rows.find((action) => !action.disabled)?.id ?? ''
  const selectedValue = rows.some(({ id }) => id === selected) ? selected : firstEnabled
  const selectedAction = filterStep ? undefined : rows.find(({ id }) => id === selectedValue)

  const closeMenu = (message?: string) => {
    menu.close()
    if (message) setNotice(message)
    input.current?.focus()
  }
  const run = (action: MenuAction) => {
    if (action.disabled) return
    if (action.keepsOpen) {
      void action.run()
      return
    }
    onOpenChange(false)
    recordQueryPick(query, action.id)
    if (action.registeredAction && onRunAction) {
      onRunAction(action.registeredAction)
      return
    }
    recordActionUsage(action.id)
    void Promise.resolve(action.run()).catch(() => undefined)
  }
  const runFollowUp = (target: MenuAction, followUp: RowAction) => {
    recordQueryPick(query, target.id)
    recordActionUsage(target.id)
    if (followUp.notice) {
      void Promise.resolve(followUp.run()).catch(() => undefined)
      closeMenu(followUp.notice)
      return
    }
    onOpenChange(false)
    void Promise.resolve(followUp.run()).catch(() => undefined)
  }
  const toggleFavoriteFor = (target: MenuAction) => {
    if (target.ephemeral) return
    const next = toggleFavorite(favorites, target.id)
    setFavorites(next)
    savePaletteFavorites(next)
    closeMenu(next.includes(target.id) ? 'Added to Favorites' : 'Removed from Favorites')
  }
  const moveFavoriteFor = (target: MenuAction, offset: -1 | 1) => {
    const next = moveFavorite(favorites, target.id, offset)
    setFavorites(next)
    savePaletteFavorites(next)
    dispatch({ type: 'select', id: target.id })
    closeMenu()
  }
  const menuItems = selectedAction
    ? menuRows(
        selectedAction,
        favorites,
        {
          run,
          runFollowUp,
          toggleFavorite: toggleFavoriteFor,
          moveFavorite: moveFavoriteFor,
          resetRanking: (target) => {
            forgetActionUsage(target.id)
            forgetQueryPicks(target.id)
            setRankingVersion((version) => version + 1)
            closeMenu(`Ranking reset for ${target.label}`)
          },
        },
        { star: Star, reset: RefreshCw },
      )
    : []
  const menu = useActionsMenu(menuItems, { dialog, list }, selectedValue, () => closeMenu())

  // Ghost text only ever completes the highlighted row, so Tab and Enter agree.
  const completion =
    askMode || filterStep || tab.id !== 'all' || !selectedAction
      ? undefined
      : autofillCompletion(query, [selectedAction.label])

  useLayoutEffect(() => {
    if (list.current) list.current.scrollTop = 0
  }, [query, tabId, filterStep, chips, mode, open])

  useEffect(() => {
    if (!notice) return undefined
    const timer = window.setTimeout(() => setNotice(undefined), 1800)
    return () => window.clearTimeout(timer)
  }, [notice])

  useEffect(() => {
    const element = dialog.current
    if (!element) return undefined
    if (open && !element.open) {
      element.showModal()
      input.current?.focus()
    } else if (!open && element.open) {
      element.close()
    }
    return () => {
      if (element.open) element.close()
    }
  }, [open])

  const switchMode = (next: PaletteMode, nextQuery = '') => {
    dispatch({ type: 'switchMode', mode: next, query: nextQuery })
    menu.close()
    ask.reset()
    paletteMode.save(next)
    input.current?.focus()
  }

  const onInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (menu.open) {
      menu.onKeyDown(event)
      return
    }
    const atStart =
      event.currentTarget.selectionStart === 0 && event.currentTarget.selectionEnd === 0
    const atEnd = event.currentTarget.selectionStart === query.length
    // Esc steps back one level: filter picker, then the query, then the palette. Handling it on
    // keydown, not in the dialog's cancel event, keeps browsers from closing it on a repeated Esc.
    if (event.key === 'Escape' && (filterStep || verbStep || query)) {
      event.preventDefault()
      dispatch({ type: 'clear' })
      return
    }
    if (event.key === 'Backspace' && !query) {
      if (askMode && !filterStep) {
        event.preventDefault()
        switchMode('search')
      } else if (filterStep || verbStep || chips.length) {
        event.preventDefault()
        dispatch({ type: 'back' })
      }
      return
    }
    if (acceptAutofill(event, completion, (text) => dispatch({ type: 'complete', query: text })))
      return
    if (askMode) {
      if (event.key === 'Enter') {
        event.preventDefault()
        void ask.ask(trimmedQuery)
      }
      return
    }
    const plainArrow = !event.metaKey && !event.altKey && !event.ctrlKey && !event.shiftKey
    if (!filterStep && plainArrow && event.key === 'ArrowRight' && atEnd) {
      event.preventDefault()
      dispatch({ type: 'stepTab', offset: 1 })
      return
    }
    if (!filterStep && plainArrow && event.key === 'ArrowLeft' && atStart) {
      event.preventDefault()
      dispatch({ type: 'stepTab', offset: -1 })
      return
    }
    const target = selectedAction
    if (!target) return
    if ((event.key === 'Enter' && event.altKey) || plainMeta(event, 'k')) {
      event.preventDefault()
      event.stopPropagation()
      menu.openMenu()
    } else if (event.key === 'Enter' && event.metaKey && target.secondary) {
      event.preventDefault()
      runFollowUp(target, target.secondary)
    } else if (plainMeta(event, 'i') && rowObjectId(target.id)) {
      // Details: the selected object's raw view.
      event.preventDefault()
      onOpenChange(false)
      showObject(rowObjectId(target.id)!)
    } else if (event.key === '.' && !query && !event.metaKey && !event.altKey) {
      // `.` lists the selected row's verbs.
      event.preventDefault()
      menu.openMenu()
    } else if (plainMeta(event, 'd') && !target.ephemeral) {
      event.preventDefault()
      toggleFavoriteFor(target)
    } else if (
      event.metaKey &&
      event.altKey &&
      (event.key === 'ArrowUp' || event.key === 'ArrowDown') &&
      favoriteIds.has(target.id)
    ) {
      event.preventDefault()
      moveFavoriteFor(target, event.key === 'ArrowUp' ? -1 : 1)
    }
  }

  const onQueryChange = (value: string) => {
    menu.close()
    if (askMode) ask.reset()
    // A leading `?` asks Brief.
    else if (!filterStep && !query && value.startsWith('?'))
      return switchMode('ask', value.slice(1).trimStart())
    dispatch({ type: 'type', query: value })
  }

  const askStatus = ask.statusFor(trimmedQuery)
  const emptyLabel = filterStep
    ? 'No matching filter.'
    : askingVerb?.object
      ? `No matching ${objectKinds[askingVerb.object].plural.toLowerCase()}`
      : tab.id === 'all'
        ? 'No results found'
        : `No results in ${tab.label}`
  const resultCount = rows.length

  return (
    <dialog
      ref={dialog}
      className="command-dialog command-palette-dialog"
      aria-labelledby="command-title"
      onCancel={(event) => {
        event.preventDefault()
        if (menu.open) closeMenu()
        else onOpenChange(false)
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onOpenChange(false)
      }}
    >
      <h2 id="command-title" className="sr-only">
        Quick actions
      </h2>
      <div className="command-modes" role="group" aria-label="Palette mode">
        {(['search', 'ask'] as const).map((value) => (
          <button
            type="button"
            key={value}
            aria-pressed={mode === value}
            onClick={() => switchMode(value)}
          >
            {value === 'search' ? 'Search' : 'Ask'}
          </button>
        ))}
      </div>
      <div className="command-popup">
        <Command
          label="Quick actions"
          shouldFilter={false}
          value={selectedValue}
          onValueChange={(id) => dispatch({ type: 'select', id })}
        >
          <label className="command-input-wrap">
            <span className="sr-only">{askMode ? 'Ask Brief' : 'Search quick actions'}</span>
            {chips.map((chip) => (
              <span className="command-chip" key={chip.field}>
                {chip.label}
                <button
                  type="button"
                  aria-label={`Remove ${chip.label} filter`}
                  onClick={() => {
                    dispatch({ type: 'removeChip', field: chip.field })
                    input.current?.focus()
                  }}
                >
                  <X size={12} aria-hidden="true" />
                </button>
              </span>
            ))}
            {filterStep && filterStep !== 'field' ? (
              <span className="command-chip command-chip-pending">
                {filterFields.find(({ field }) => field === filterStep)?.label}
              </span>
            ) : null}
            {askingVerb ? (
              <span className="command-chip command-chip-pending">
                {askingVerb.title.replace(/…$/, '')}
              </span>
            ) : null}
            <AutofillField query={query} completion={completion}>
              <Command.Input
                ref={input}
                value={query}
                onKeyDown={onInputKeyDown}
                onValueChange={onQueryChange}
                placeholder={
                  askMode
                    ? 'Ask about your finances…'
                    : askingVerb?.object
                      ? `Which ${objectKinds[askingVerb.object].label.toLowerCase()}?`
                      : filterStep === 'field'
                        ? 'Filter by…'
                        : filterStep
                          ? 'Choose a value…'
                          : 'Search'
                }
              />
            </AutofillField>
            <button
              type="button"
              className="command-close"
              aria-label="Close"
              onClick={() => onOpenChange(false)}
            >
              <X size={15} aria-hidden="true" />
            </button>
          </label>
          {!askMode && !filterStep && !askingVerb ? (
            <div className="command-tabs" role="tablist" aria-label="Result type">
              {paletteTabs.map((option) => (
                <button
                  type="button"
                  role="tab"
                  key={option.id}
                  aria-selected={option.id === tab.id}
                  tabIndex={-1}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => dispatch({ type: 'showTab', tabId: option.id })}
                >
                  {option.label}
                </button>
              ))}
            </div>
          ) : null}
          {askMode ? (
            <div className="command-ask" aria-live="polite">
              {ask.answer ? (
                <div className="command-answer">
                  <strong>Brief</strong>
                  <p data-outcome={ask.answer.outcome}>{ask.answer.text}</p>
                  {ask.answer.unverified.length ? (
                    <small className="command-unverified">
                      Not in the figures it looked up:{' '}
                      <span className="sensitive-value">{ask.answer.unverified.join(', ')}</span>
                    </small>
                  ) : null}
                  {ask.answer.evidence.length ? (
                    <section className="command-evidence" aria-label="Figures behind this answer">
                      <h3>Based on</h3>
                      {ask.answer.evidence.map((call, index) => {
                        const view = ledgerView(call)
                        const pinned = isPinned(pins, call.name, call.arguments)
                        const question = ask.answer?.question ?? ''
                        return (
                          <details key={index} open={index === 0}>
                            <summary>
                              {view.title}
                              {view.scope ? <small>{view.scope}</small> : null}
                            </summary>
                            <LedgerResult call={call} />
                            <button
                              type="button"
                              className="button-base button-secondary button-compact"
                              disabled={pinned}
                              onClick={() =>
                                savePins(
                                  withPin(pins, {
                                    id: crypto.randomUUID(),
                                    question,
                                    name: call.name,
                                    arguments: call.arguments,
                                  }),
                                )
                              }
                            >
                              {pinned ? 'Pinned to Home' : 'Pin to Home'}
                            </button>
                          </details>
                        )
                      })}
                    </section>
                  ) : (
                    <small className="command-evidence-none">
                      This answer cited no saved figures.
                    </small>
                  )}
                </div>
              ) : ask.error ? (
                <p className="command-ask-status" role="alert">
                  {ask.error}
                </p>
              ) : askStatus ? (
                <p className="command-ask-status">{askStatus}</p>
              ) : null}
            </div>
          ) : (
            <Command.List ref={list} className="command-list" label="Quick actions">
              <Command.Empty className="command-empty">
                <span role="status">{emptyLabel}</span>
                {tab.id !== 'all' && !filterStep ? (
                  <button
                    type="button"
                    className="command-empty-action"
                    onClick={() => {
                      dispatch({ type: 'showTab', tabId: 'all' })
                      input.current?.focus()
                    }}
                  >
                    Search all
                  </button>
                ) : null}
              </Command.Empty>
              {sections.map(({ heading, items }, sectionIndex) =>
                items.length ? (
                  <Command.Group
                    className="command-group"
                    heading={heading}
                    key={heading ?? sectionIndex}
                  >
                    {items.map((action) => {
                      const isSelected = action.id === selectedValue
                      return (
                        <Command.Item
                          key={action.id}
                          className={action.detail ? 'command-result-item' : undefined}
                          value={action.id}
                          disabled={action.disabled}
                          aria-busy={
                            action.id === 'refresh' || action.id === 'finance.refresh'
                              ? isRefreshing
                              : undefined
                          }
                          onSelect={() => run(action)}
                        >
                          <RowMark action={action} />
                          <span className="command-item-copy">
                            <span>
                              <Highlighted text={action.label} ranges={action.ranges} />
                            </span>
                            {action.detail ? <small>{action.detail}</small> : null}
                          </span>
                          <span className="command-item-meta">
                            {favoriteIds.has(action.id) ? (
                              <Star className="command-favorite" size={12} aria-label="Favorite" />
                            ) : null}
                            {isSelected && !action.keepsOpen ? (
                              <button
                                type="button"
                                className="command-more"
                                aria-label={`Actions for ${action.label}`}
                                aria-expanded={menu.open}
                                tabIndex={-1}
                                onMouseDown={(event) => event.preventDefault()}
                                onClick={(event) => {
                                  event.stopPropagation()
                                  menu.toggle()
                                }}
                              >
                                <Ellipsis size={14} aria-hidden="true" />
                              </button>
                            ) : null}
                            {isSelected ? (
                              <Keys label="↵" />
                            ) : action.shortcut ? (
                              <Keys label={action.shortcut} />
                            ) : null}
                          </span>
                        </Command.Item>
                      )
                    })}
                  </Command.Group>
                ) : null,
              )}
            </Command.List>
          )}
          <span className="sr-only" role="status" aria-live="polite">
            {askMode ? '' : resultCount ? countOf(resultCount, 'result') : ''}
          </span>
          <div className="command-footer">
            {notice ? (
              <span className="command-footer-notice" role="status">
                {notice}
              </span>
            ) : askMode ? (
              <>
                <FooterHint label="Close" keys={['Esc']} />
                {!query ? <FooterHint label="Search" keys={['⌫']} /> : null}
                {askReady ? <FooterHint label="Ask" keys={['↵']} /> : null}
              </>
            ) : filterStep || askingVerb ? (
              <>
                <FooterHint label="Close" keys={['Esc']} />
                <FooterHint label="Back" keys={['⌫']} />
                <FooterHint label="Choose" keys={['↵']} />
              </>
            ) : (
              <>
                <FooterHint label="Close" keys={['Esc']} />
                <FooterHint label="Change type" keys={['←', '→']} />
                <FooterHint label="Filters" keys={['/']} />
                {selectedAction ? <FooterHint label="Actions" keys={['⌥↵']} /> : null}
              </>
            )}
          </div>
        </Command>
      </div>
      {menu.open && selectedAction ? (
        <ActionsPopover
          target={selectedAction}
          items={menuItems}
          index={menu.index}
          position={menu.position}
          onHover={menu.setIndex}
        />
      ) : null}
    </dialog>
  )
}
