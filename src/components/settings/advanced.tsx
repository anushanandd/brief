import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { getAppVersion, getLoginItem, isTauri, setLoginItem } from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { financeSettings } from '../../lib/finance-settings'
import {
  getMarketUpdateInterval,
  marketUpdateIntervals,
  saveMarketUpdateInterval,
} from '../../lib/market-preferences'
import { advancedPreferenceKeys } from '../../lib/settings-preferences'
import { ChevronRight, Palette, ScrollText, ShieldCheck } from '../icons'
import { Keys } from '../keys'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
  SettingsSwitch,
} from '../settings-ui'
import { ChartRangeSelect } from '../ui'
import { CalendarSyncCard } from './calendar'

export function MarketSection() {
  const [marketUpdateInterval, setMarketUpdateInterval] = useState(getMarketUpdateInterval)
  return (
    <SettingsSection
      id="market"
      title="Market data"
      action={<SettingsResetButton section="market data" keys={advancedPreferenceKeys} />}
    >
      <SettingsCard>
        <SettingsRow
          id="live-price-updates"
          title="Live price updates"
          description="How often quotes redraw."
        >
          <ChartRangeSelect
            label="Live price updates"
            options={marketUpdateIntervals.map(({ seconds, label, settingsLabel }) => ({
              value: seconds,
              label,
              accessibleLabel: settingsLabel,
            }))}
            value={marketUpdateInterval}
            onValueChange={(seconds) => {
              saveMarketUpdateInterval(seconds)
              setMarketUpdateInterval(seconds)
            }}
          />
        </SettingsRow>
      </SettingsCard>
    </SettingsSection>
  )
}

/** Data stays current without the window open: a daily refresh by this Mac, no server. */
function BackgroundRefreshCard() {
  const finance = useFinance()
  const { updateSettings } = useFinanceEdits()
  const loginItem = useQuery({ queryKey: ['login-item'], queryFn: getLoginItem })
  const queryClient = useQueryClient()
  const enabled = financeSettings(finance.data).backgroundRefresh
  const unavailable = !isTauri() || Boolean(finance.data?.recovery)
  const setLogin = useMutation({
    mutationFn: setLoginItem,
    onSuccess: (state) => queryClient.setQueryData(['login-item'], state),
    onError: (error) =>
      toast.error('Could not change Login Items', {
        description: errorMessage(error),
      }),
  })
  return (
    <SettingsCard>
      <SettingsRow
        id="background-refresh"
        title="Background refresh"
        description="Daily, with the window closed. Adds a menu bar item."
      >
        <SettingsSwitch
          labelledBy="background-refresh-title"
          describedBy="background-refresh-description"
          checked={enabled}
          disabled={unavailable}
          onCheckedChange={(checked) =>
            void updateSettings((current) => ({ ...current, backgroundRefresh: checked })).catch(
              (error) =>
                toast.error('Could not save background refresh', {
                  description: errorMessage(error),
                }),
            )
          }
        />
      </SettingsRow>
      <SettingsRow
        id="notifications"
        title="Notifications"
        description="Payments due, cash shortfalls, expiring credits. Never amounts."
      >
        <SettingsSwitch
          labelledBy="notifications-title"
          describedBy="notifications-description"
          checked={financeSettings(finance.data).notifications}
          disabled={unavailable || !enabled}
          onCheckedChange={(checked) =>
            void updateSettings((current) => ({ ...current, notifications: checked })).catch(
              (error) =>
                toast.error('Could not save notifications', {
                  description: errorMessage(error),
                }),
            )
          }
        />
      </SettingsRow>
      <SettingsRow
        id="global-shortcut"
        title="System-wide shortcut"
        description="Brings Brief forward with the command palette open, from any app."
      >
        <Keys label="⇧⌘B" />
      </SettingsRow>
      <SettingsRow
        id="open-at-login"
        title="Open at login"
        description={
          loginItem.data === 'requires-approval'
            ? 'Allow Brief in System Settings → General → Login Items.'
            : 'Keeps background refresh running after a restart.'
        }
      >
        <SettingsSwitch
          labelledBy="open-at-login-title"
          describedBy="open-at-login-description"
          checked={loginItem.data === 'enabled' || loginItem.data === 'requires-approval'}
          disabled={!isTauri() || loginItem.data === 'unavailable' || setLogin.isPending}
          onCheckedChange={(checked) => setLogin.mutate(checked)}
        />
      </SettingsRow>
    </SettingsCard>
  )
}

export function AdvancedSection() {
  const version = useQuery({ queryKey: ['app-version'], queryFn: getAppVersion })

  return (
    <SettingsSection id="advanced" title="Advanced">
      <BackgroundRefreshCard />
      <CalendarSyncCard />
      <SettingsCard>
        <SettingsBlock id="diagnostics" title="Diagnostics">
          <div className="settings-destination-list">
            <Link to="/health" className="settings-destination-row">
              <ShieldCheck size={16} aria-hidden="true" />
              <span>
                <strong>Data health</strong>
                <small>Freshness, coverage, and reconciliation checks</small>
              </span>
              <ChevronRight size={15} aria-hidden="true" />
            </Link>
            <Link to="/logs" className="settings-destination-row">
              <ScrollText size={16} aria-hidden="true" />
              <span>
                <strong>Diagnostics</strong>
                <small>Sync history, runtime state, and source times</small>
              </span>
              <ChevronRight size={15} aria-hidden="true" />
            </Link>
            {import.meta.env.DEV ? (
              <Link to="/settings/design" className="settings-destination-row">
                <Palette size={16} aria-hidden="true" />
                <span>
                  <strong>Design</strong>
                  <small>Tokens and shared components</small>
                </span>
                <ChevronRight size={15} aria-hidden="true" />
              </Link>
            ) : null}
          </div>
        </SettingsBlock>
        <SettingsBlock id="about" title="About Brief">
          <div className="about-settings">
            <div>
              <span>Version</span>
              <strong>{version.data ?? (version.isLoading ? 'Loading…' : 'Unavailable')}</strong>
            </div>
            <div>
              <span>Hosted backend</span>
              <strong>None</strong>
            </div>
            {/* Places read from bank descriptors use this bundled city list. */}
            <div>
              <span>City data</span>
              <strong>GeoNames · CC BY 4.0</strong>
            </div>
            {/* Businesses matched to purchases come from Overture's places release. */}
            <div>
              <span>Place data</span>
              <strong>Overture Maps · CDLA-Permissive-2.0, Apache-2.0, CC0</strong>
            </div>
          </div>
        </SettingsBlock>
      </SettingsCard>
    </SettingsSection>
  )
}
