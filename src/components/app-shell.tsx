import { Outlet, useLocation } from '@tanstack/react-router'
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { createPortal } from 'react-dom'

import { useAccountJumpActions } from '../hooks/use-account-jump-actions'
import { useAppUpdateChecks } from '../hooks/use-app-update'
import { useFinance } from '../hooks/use-finance'
import { useFocusRegion } from '../hooks/use-focus-region'
import { useGlobalActions } from '../hooks/use-global-actions'
import { useInvestmentsWarmup } from '../hooks/use-investments-warmup'
import { useKeyboardDispatch } from '../hooks/use-keyboard-dispatch'
import { useNativeCommands } from '../hooks/use-native-commands'
import { useAutoRefreshFinance, useWalletWatch } from '../hooks/use-refresh-finance'
import { useRegisterActions, useRegisteredActions } from '../lib/action-system'
import { analyticsChart, analyticsCharts, analyticsGroup } from '../lib/analytics'
import { isTauri } from '../lib/api'
import { announceAmountsHidden, setAmountsHidden } from '../lib/format'
import { openModal } from '../lib/keyboard'
import { useOrderedNavigation } from '../lib/navigation-preferences'
import { getPrivacyPreferences, usePrivacyPreferences } from '../lib/privacy-preferences'
import { useSettingsDialog } from '../lib/settings-dialog'
import { CommandKeyHints } from './command-key-hints'
import { CommandMenu } from './command-menu'
import { FocusHints } from './focus-hints'
import { KeyGuide } from './key-guide'
import { ObjectDetailsProvider } from './object-details'
import { RecoveryNotice } from './recovery-notice'
import { SettingsDialog } from './settings-dialog'
import { ShortcutsDialog } from './shortcuts-dialog'
import { Sidebar } from './sidebar'

/**
 * The window: sidebar, page, and the overlays every page shares. Brief is driven by the pointer
 * and keyboard shortcuts, so the shell owns the global actions and dispatches key sequences
 * to whichever action, global or registered by the page, they name. Any object's raw view
 * opens over all of it.
 */
export function AppShell() {
  return (
    <ObjectDetailsProvider>
      <Shell />
    </ObjectDetailsProvider>
  )
}

function Shell() {
  const pages = useOrderedNavigation()
  const pathname = useLocation({ select: (location) => location.pathname })
  const reportChart = useLocation({
    select: (location) =>
      location.pathname === '/analytics' ? analyticsChart(location.search.chart) : undefined,
  })
  // A report belongs to a destination, which stays the highlighted page while it is open.
  const reportHome =
    pathname === '/analytics'
      ? analyticsGroup(reportChart ?? analyticsCharts[0].id) === 'money'
        ? '/spending'
        : '/holdings'
      : undefined
  const mainContentRef = useRef<HTMLElement>(null)
  const settings = useSettingsDialog()
  const finance = useFinance()
  const [commandOpen, setCommandOpen] = useState(false)
  const [shortcutsOpen, setShortcutsOpen] = useState(false)
  const [focusHintsOpen, setFocusHintsOpen] = useState(false)
  const privacyPreferences = usePrivacyPreferences()
  const [valuesHidden, setValuesHidden] = useState(() => getPrivacyPreferences().hideOnLaunch)
  // Formatters read this as they run, so it's set before anything below renders. Views the
  // shell doesn't render again, such as the routed page, hear of the change just after.
  setAmountsHidden(valuesHidden)
  useLayoutEffect(announceAmountsHidden, [valuesHidden])
  useAutoRefreshFinance(
    finance.data?.updatedAt,
    isTauri() &&
      !finance.data?.recovery &&
      !finance.integrationStatusLoading &&
      Object.values(finance.integrationStatus).some(Boolean),
  )
  useWalletWatch(
    isTauri() && !finance.data?.recovery && Boolean(finance.data?.settings?.appleWallet),
  )
  useInvestmentsWarmup(
    finance.data?.recovery ? undefined : finance.data,
    finance.integrationStatus.alpaca,
  )

  const focusRegion = useFocusRegion()
  const registeredActions = useRegisteredActions()
  // The guide opens through the dispatcher below; the action only needs a stable callback.
  const openKeyGuide = useRef(() => {})
  const globalActions = useGlobalActions({
    openPalette: useCallback(() => setCommandOpen(true), []),
    openShortcuts: useCallback(() => setShortcutsOpen(true), []),
    openKeyGuide: useCallback(() => openKeyGuide.current(), []),
    openFocusHints: useCallback(() => setFocusHintsOpen(true), []),
    valuesHidden,
    toggleValues: useCallback(() => setValuesHidden((hidden) => !hidden), []),
  })
  useRegisterActions(globalActions)
  useAccountJumpActions()
  const menuActions = useMemo(
    () => [
      ...new Map(
        [...globalActions, ...registeredActions].map((action) => [action.id, action]),
      ).values(),
    ],
    [globalActions, registeredActions],
  )
  const keyboard = useKeyboardDispatch(menuActions, focusRegion, commandOpen || focusHintsOpen)
  useLayoutEffect(() => {
    openKeyGuide.current = keyboard.openKeyGuide
  })
  useNativeCommands(menuActions, keyboard.executeAction)
  useAppUpdateChecks()

  useLayoutEffect(() => {
    document.documentElement.toggleAttribute('data-values-hidden', valuesHidden)
    return () => document.documentElement.removeAttribute('data-values-hidden')
  }, [valuesHidden])
  useEffect(() => {
    if (!privacyPreferences.hideOnBlur) return undefined
    const hide = () => setValuesHidden(true)
    window.addEventListener('blur', hide)
    return () => window.removeEventListener('blur', hide)
  }, [privacyPreferences.hideOnBlur])
  useLayoutEffect(() => {
    if (mainContentRef.current) mainContentRef.current.scrollTop = 0
  }, [pathname])
  // Across the page, Tab never walks focus between controls: the sidebar, strips, and lists are
  // reached by shortcuts and arrows instead. Inside a dialog Tab walks its controls, which the
  // native modal keeps within it. Fields that use Tab themselves, such as autofill, handle it
  // first.
  useEffect(() => {
    const blockTab = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return
      if (event.target instanceof Element && event.target.closest('dialog[open]')) return
      event.preventDefault()
    }
    window.addEventListener('keydown', blockTab)
    return () => window.removeEventListener('keydown', blockTab)
  }, [])

  return (
    <>
      <div className="app-frame">
        <div className="window-drag-region" data-tauri-drag-region aria-hidden="true" />
        <Sidebar
          pages={pages}
          reportHome={reportHome}
          settingsOpen={settings.open}
          onOpenSettings={settings.show}
        />

        <main className="main-content" ref={mainContentRef}>
          {finance.data?.recovery ? <RecoveryNotice recovery={finance.data.recovery} /> : null}
          {finance.data?.syncWarnings?.length ? (
            <p className="sync-notice" role="status">
              {finance.data.syncWarnings.join(' · ')}
            </p>
          ) : null}
          {finance.annotationWarning ? (
            <p className="sync-notice" role="status">
              {finance.annotationWarning}
            </p>
          ) : null}
          {finance.data?.recovery && !finance.data.recovery.canRestore ? null : <Outlet />}
        </main>

        {/* Settings opens over any page and stays usable when the snapshot can't load. */}
        <SettingsDialog />

        {commandOpen ? (
          <CommandMenu
            open
            onOpenChange={setCommandOpen}
            onShowShortcuts={() => setShortcutsOpen(true)}
            onRunAction={keyboard.executeAction}
          />
        ) : null}
        {keyboard.keyGuideOpen ? (
          <OverModal>
            <KeyGuide
              actions={keyboard.guideActions}
              prefix={keyboard.keySequence}
              onRun={keyboard.executeAction}
              onClose={keyboard.closeKeyState}
              onShowAll={() => {
                keyboard.closeKeyState()
                setShortcutsOpen(true)
              }}
            />
          </OverModal>
        ) : null}
        <ShortcutsDialog
          open={shortcutsOpen}
          actions={keyboard.actions}
          onClose={() => setShortcutsOpen(false)}
        />
        {focusHintsOpen ? (
          <OverModal>
            <FocusHints onClose={() => setFocusHintsOpen(false)} />
          </OverModal>
        ) : null}
        <CommandKeyHints
          actions={keyboard.actions}
          paused={
            commandOpen || focusHintsOpen || shortcutsOpen || keyboard.keyGuideOpen || settings.open
          }
        />
      </div>
    </>
  )
}

/**
 * An overlay opened over a modal dialog renders inside it: the dialog sits in the top layer,
 * above anything rendered in the page, and leaves the page inert.
 */
function OverModal({ children }: { children: ReactNode }) {
  const [host] = useState(() => {
    const modal = openModal()
    return modal instanceof HTMLDialogElement ? modal : undefined
  })
  return host ? createPortal(children, host) : children
}
