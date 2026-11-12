import { useMemo, useState } from 'react'

import { useNumberKeyDestinations } from '../../hooks/use-number-keys'
import { useRegisteredActions } from '../../lib/action-system'
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
  assignNumberKey,
  isDestination,
  numberKeyPreference,
  numberKeys,
  resolveNumberKeys,
  targetValue,
} from '../../lib/number-keys'
import {
  chartPreferenceKeys,
  navigationPreferenceKeys,
  themePreferenceKeys,
} from '../../lib/settings-preferences'
import { FilterSelect } from '../filter-select'
import { ArrowDown, ArrowUp, RotateCcw } from '../icons'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
} from '../settings-ui'
import { Button, ChartRangeSelect } from '../ui'

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
  const accountGroupsMode = useAccountGroupsMode()
  return (
    <SettingsSection
      id="appearance"
      title="Appearance"
      action={<SettingsResetButton section="appearance" keys={themePreferenceKeys} />}
    >
      <SettingsCard>
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
        <NumberKeysBlock />
      </SettingsCard>
    </SettingsSection>
  )
}

/**
 * What each number key opens: a page, report, account, type of account, or held security, or
 * nothing. The list is long, so it is grouped, and typing jumps through it. A saved destination
 * that no longer exists stays chosen, named as it was and marked unavailable, until changed.
 */
function NumberKeysBlock() {
  const actions = useRegisteredActions()
  const pages = useMemo(() => actions.filter(({ id }) => isDestination(id)), [actions])
  const destinations = useNumberKeyDestinations(pages)
  const choices = numberKeyPreference.use()
  const resolved = resolveNumberKeys(choices, destinations)
  const options = destinations.map(({ value, label, section }) => ({
    value,
    label,
    group: section,
  }))
  return (
    <SettingsBlock
      id="number-keys"
      title="Number keys"
      description="Each opens what you choose from anywhere but a text field. A key with nothing chosen does nothing."
      action={
        <Button
          icon={RotateCcw}
          size="compact"
          variant="ghost"
          onClick={() => numberKeyPreference.save({})}
        >
          Reset
        </Button>
      }
    >
      <ol className="settings-list">
        {numberKeys.map((key) => {
          const saved = choices[key]
          const gone = saved && !resolved[key]
          return (
            <li className="settings-list-row" key={key} data-keyboard-row tabIndex={-1}>
              <kbd className="settings-list-key">{key}</kbd>
              <FilterSelect
                label={`What the ${key} key opens`}
                className="settings-number-key-select"
                keyboardOpen
                value={saved ? targetValue(saved) : ''}
                options={[
                  { value: '', label: 'None' },
                  ...(gone
                    ? [
                        {
                          value: targetValue(saved),
                          label: `${saved.name ?? 'This destination'} · no longer available`,
                        },
                      ]
                    : []),
                  ...options,
                ]}
                onValueChange={(value) =>
                  numberKeyPreference.save(
                    assignNumberKey(
                      choices,
                      key,
                      destinations.find((destination) => destination.value === value)?.target ??
                        null,
                    ),
                  )
                }
              />
            </li>
          )
        })}
      </ol>
    </SettingsBlock>
  )
}
