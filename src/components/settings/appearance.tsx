import { useTheme } from 'next-themes'
import { useState } from 'react'

import {
  getDefaultGraphWindow,
  graphWindows,
  saveDefaultGraphWindow,
} from '../../lib/graph-preferences'
import {
  accountGroupsModes,
  saveAccountGroupsMode,
  useAccountGroupsMode,
} from '../../lib/home-preferences'
import { movePage, savePageOrder, useOrderedNavigation } from '../../lib/navigation-preferences'
import {
  chartPreferenceKeys,
  navigationPreferenceKeys,
  themePreferenceKeys,
} from '../../lib/settings-preferences'
import { ArrowDown, ArrowUp } from '../icons'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
} from '../settings-ui'
import { Button, ChartRangeSelect } from '../ui'

const themes = [
  { value: 'dark', label: 'Dark', accessibleLabel: 'Dark' },
  { value: 'custom', label: 'Brief', accessibleLabel: 'Brief' },
] as const

function MoveButtons({
  label,
  index,
  count,
  onMove,
}: {
  label: string
  index: number
  count: number
  onMove: (direction: -1 | 1) => void
}) {
  return (
    <span className="settings-order-buttons">
      <Button
        icon={ArrowUp}
        size="icon-compact"
        variant="ghost"
        disabled={index === 0}
        aria-label={`Move ${label} up`}
        onClick={() => onMove(-1)}
      />
      <Button
        icon={ArrowDown}
        size="icon-compact"
        variant="ghost"
        disabled={index === count - 1}
        aria-label={`Move ${label} down`}
        onClick={() => onMove(1)}
      />
    </span>
  )
}

export function AppearanceSection() {
  const { theme, setTheme } = useTheme()
  const accountGroupsMode = useAccountGroupsMode()
  return (
    <SettingsSection
      id="appearance"
      title="Appearance"
      action={<SettingsResetButton section="appearance" keys={themePreferenceKeys} />}
    >
      <SettingsCard>
        <SettingsRow id="theme" title="Theme">
          <ChartRangeSelect
            label="Theme"
            options={[...themes]}
            value={theme ?? 'dark'}
            onValueChange={setTheme}
          />
        </SettingsRow>
        <SettingsRow id="home-accounts" title="Account categories">
          <ChartRangeSelect
            label="Overview account categories"
            options={[...accountGroupsModes]}
            value={accountGroupsMode}
            onValueChange={saveAccountGroupsMode}
          />
        </SettingsRow>
      </SettingsCard>
    </SettingsSection>
  )
}

export function ChartsSection() {
  const [defaultGraphWindow, setDefaultGraphWindow] = useState(getDefaultGraphWindow)

  return (
    <SettingsSection
      id="charts"
      title="Charts"
      action={<SettingsResetButton section="chart" keys={chartPreferenceKeys} />}
    >
      <SettingsCard>
        <SettingsRow id="home-range" title="Chart range">
          <ChartRangeSelect
            label="Default chart range"
            options={graphWindows.map(({ secs, label, settingsLabel }) => ({
              value: secs,
              label,
              accessibleLabel: settingsLabel,
            }))}
            value={defaultGraphWindow}
            onValueChange={(seconds) => {
              saveDefaultGraphWindow(seconds)
              setDefaultGraphWindow(seconds)
            }}
          />
        </SettingsRow>
      </SettingsCard>
    </SettingsSection>
  )
}

export function NavigationSection() {
  const orderedPages = useOrderedNavigation()
  return (
    <SettingsSection
      id="navigation"
      title="Navigation"
      action={<SettingsResetButton section="navigation" keys={navigationPreferenceKeys} />}
    >
      <SettingsCard>
        <SettingsBlock
          id="page-order"
          title="Page order"
          description={`Also sets ⌘1–⌘${orderedPages.length}.`}
        >
          <ol className="settings-list">
            {orderedPages.map(({ to, label, icon: Icon }, index) => (
              <li className="settings-list-row" key={to}>
                <span className="settings-list-copy settings-list-copy-inline">
                  <Icon size={16} aria-hidden="true" />
                  <strong>{label}</strong>
                </span>
                <kbd className="settings-list-key">⌘{index + 1}</kbd>
                <MoveButtons
                  label={`${label} page`}
                  index={index}
                  count={orderedPages.length}
                  onMove={(direction) =>
                    savePageOrder(
                      movePage(
                        orderedPages.map((page) => page.to),
                        index,
                        direction,
                      ),
                    )
                  }
                />
              </li>
            ))}
          </ol>
        </SettingsBlock>
      </SettingsCard>
    </SettingsSection>
  )
}
