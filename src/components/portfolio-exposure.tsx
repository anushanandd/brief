import { useNavigate } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { usePortfolioSectors } from '../hooks/use-portfolio-sectors'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { isTauri } from '../lib/api'
import { formatCurrency, formatShare } from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'
import { RankedBars } from './ranked-bars'
import { Card, ChartRangeSelect, EmptyState, Metric, SectionHeading } from './ui'

type View = 'holding' | 'kind' | 'sector' | 'account'
const viewOptions: Array<{ value: View; label: string; accessibleLabel: string }> = [
  { value: 'holding', label: 'Holding', accessibleLabel: 'By holding' },
  { value: 'kind', label: 'Kind', accessibleLabel: 'By kind' },
  { value: 'sector', label: 'Sector', accessibleLabel: 'By sector' },
  { value: 'account', label: 'Account', accessibleLabel: 'By account' },
]
// Grouping choices as commands, so the menu bar and palette can regroup the card too.
const exposureActions = (setView: (view: View) => void): AppAction[] =>
  viewOptions.map(({ value, accessibleLabel }) => ({
    id: `exposure.view.${value}`,
    title: `Show exposure ${accessibleLabel.toLowerCase()}`,
    search: 'exposure allocation group',
    group: 'View',
    scope: 'page',
    run: () => setView(value),
  }))
const dataColors = Array.from({ length: 7 }, (_, index) => `var(--data-${index + 1})`)

/**
 * Where the money sits, and the concentrations worth knowing about. By holding, it is the
 * portfolio's list of what you own, and a row opens that holding.
 */
export function PortfolioExposure({ data }: { data: FinanceSnapshot }) {
  const native = typeof window !== 'undefined' && isTauri()
  const navigate = useNavigate()
  const [view, setView] = useState<View>('holding')
  useRegisterActions(useMemo(() => exposureActions(setView), []))
  const allocation = data.insights?.allocation
  const sectors = usePortfolioSectors(data, Boolean(allocation))
  if (!allocation)
    return (
      <Card className="portfolio-card">
        <SectionHeading title="Exposure" />
        <EmptyState>No positions with a USD value.</EmptyState>
      </Card>
    )
  // Concentration, in one holding or one sector, is flagged under Needs you.
  const bars = (items: Array<{ name: string; value: number }>) =>
    items.map((item, index) => ({
      ...item,
      // Past seven, rows share the neutral series colour instead of a generated hue.
      color: dataColors[index] ?? 'var(--chart-secondary)',
    }))
  return (
    <Card className="portfolio-card portfolio-exposure-card">
      <SectionHeading
        title="Exposure"
        action={
          <ChartRangeSelect
            label="Exposure grouping"
            command="exposure.view."
            value={view}
            options={viewOptions}
            onValueChange={setView}
          />
        }
      />
      <div className="account-summary-metrics">
        <Metric label="Largest position" value={formatShare(allocation.largestPercent)} />
        <Metric label="Top five" value={formatShare(allocation.topFivePercent)} />
      </div>
      {view === 'holding' ? (
        <RankedBars
          items={bars(allocation.securities.map(({ key, value }) => ({ name: key, value })))}
          label="Investments by holding"
          favorable="increase"
          limit={12}
          emptyMessage="No known positions."
          onSelect={(ticker) => {
            if (ticker) void navigate({ to: '/holdings/$ticker', params: { ticker } })
          }}
        />
      ) : view === 'kind' ? (
        <RankedBars
          items={bars(
            (allocation.kinds.length ? allocation.kinds : allocation.securities).map((slice) => ({
              name: allocation.kinds.length ? slice.label : slice.key,
              value: slice.value,
            })),
          )}
          label="Investments by kind"
          favorable="increase"
          emptyMessage="No known positions."
        />
      ) : view === 'account' ? (
        <RankedBars
          items={bars(allocation.accounts.map(({ label, value }) => ({ name: label, value })))}
          label="Investments by account"
          favorable="increase"
          emptyMessage="No known positions."
        />
      ) : !native ? (
        <EmptyState>Sectors need the desktop app.</EmptyState>
      ) : sectors.isLoading ? (
        <p className="coming-up-quiet" role="status">
          Classifying stocks…
        </p>
      ) : sectors.isError || !sectors.data ? (
        <EmptyState>Sectors are unavailable right now.</EmptyState>
      ) : sectors.data.needsContact ? (
        <EmptyState>Add an SEC contact email in Settings to see sectors.</EmptyState>
      ) : (
        <>
          <RankedBars
            items={bars(sectors.data.sectors.map(({ label, value }) => ({ name: label, value })))}
            label="Stocks by sector"
            favorable="increase"
            emptyMessage="No stocks with a known sector."
          />
          <p className="portfolio-note">
            {sectors.data.funds ? (
              <>
                Funds and other assets, not broken down:{' '}
                <span className="sensitive-value">{formatCurrency(sectors.data.funds)}</span>.{' '}
              </>
            ) : null}
            {sectors.data.unclassified ? (
              <>
                Stocks without a sector:{' '}
                <span className="sensitive-value">{formatCurrency(sectors.data.unclassified)}</span>
                .{' '}
              </>
            ) : null}
            Sectors from SEC industry codes; approximate.
          </p>
        </>
      )}
      {allocation.incomplete || allocation.cashUnknown ? (
        <p className="portfolio-note">
          {[
            allocation.incomplete ? 'Excludes positions without a USD value' : '',
            allocation.cashUnknown ? 'Excludes unknown cash' : '',
          ]
            .filter(Boolean)
            .join('. ')}
          .
        </p>
      ) : null}
    </Card>
  )
}
