import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useMemo, type ReactNode } from 'react'

import { accountGroups } from '../lib/account-groups'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { getDataHealth } from '../lib/api'
import { financeSettings } from '../lib/finance-settings'
import { countOf, formatCurrency, formatShare } from '../lib/format'
import { chartPeriod } from '../lib/graph-preferences'
import { sumCents } from '../lib/money'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { AccountGroupList } from './account-groups'
import { ExpectedActivity } from './expected-activity'
import { Button, Card, EmptyState, SectionHeading, StatusDot } from './ui'

const groupColors = ['var(--data-1)', 'var(--data-4)', 'var(--data-3)', 'var(--data-2)']

/**
 * Every account at once: the combined chart, any connection that needs attention, each account
 * under its group beside recent activity, and a side column with the combined figures, what the
 * total is made of, and what's coming up.
 */
export function AllAccountsOverview({
  data,
  accounts,
  graphWindow,
  chartCard,
  activityPreview,
  details,
}: {
  data: FinanceSnapshot
  accounts: Account[]
  graphWindow: number
  chartCard: ReactNode
  activityPreview: ReactNode
  /** Investment and income figures across every account. */
  details: ReactNode
}) {
  const names = financeSettings(data).accountNames
  const changeWindow = chartPeriod(graphWindow)
  const periodChanges = data.insights?.changes.find(({ window }) => window === changeWindow)
  const groups = accountGroups(accounts, periodChanges?.accounts)
  const assets = groups.filter(({ value }) => value != null && value > 0)
  const assetTotal = sumCents(assets.map(({ value }) => value!)) || null
  const liabilities = groups.filter(({ value }) => value != null && value < 0)
  const unknown = groups.filter(({ value }) => value == null)

  return (
    <div className="account-overview-dashboard all-accounts-overview">
      {/* Columns that stack on their own, so none waits on another's height. */}
      <div className="all-accounts-main">
        {chartCard}
        <ConnectionNotice revision={data.revision} />
        <div className="all-accounts-lists">
          <Card className="account-preview-card all-accounts-card">
            <SectionHeading title="Balances" />
            <div role="region" aria-label="Balances by account" data-keyboard-region>
              {groups.length ? (
                <AccountGroupList
                  groups={groups}
                  changes={periodChanges?.accounts}
                  accountNames={names}
                />
              ) : (
                <EmptyState>No connected accounts.</EmptyState>
              )}
            </div>
          </Card>
          {activityPreview}
        </div>
      </div>
      <div className="all-accounts-side">
        {details}
        <Card className="account-overview-summary-card accounts-composition-card">
          <SectionHeading title="Made of" />
          {assetTotal ? (
            <div className="accounts-composition-bar" aria-hidden="true">
              {assets.map((group, index) => (
                <i
                  key={group.type}
                  style={{
                    flexGrow: group.value!,
                    background: groupColors[index % groupColors.length],
                  }}
                />
              ))}
            </div>
          ) : null}
          <dl className="accounts-composition">
            {assets.map((group, index) => (
              <div key={group.type}>
                <dt>
                  <i
                    className="analytics-category-dot"
                    style={{ background: groupColors[index % groupColors.length] }}
                    aria-hidden="true"
                  />
                  {group.label}
                </dt>
                <dd>
                  <span className="sensitive-value">{formatCurrency(group.value)}</span>
                  {assetTotal ? (
                    <small>{formatShare((group.value! / assetTotal) * 100)}</small>
                  ) : null}
                </dd>
              </div>
            ))}
            {liabilities.map((group) => (
              <div key={group.type}>
                <dt>{group.label}</dt>
                <dd>
                  <span className="sensitive-value negative">{formatCurrency(group.value)}</span>
                </dd>
              </div>
            ))}
            {unknown.map((group) => (
              <div key={group.type}>
                <dt>{group.label}</dt>
                <dd>
                  <span className="muted">Unavailable</span>
                </dd>
              </div>
            ))}
          </dl>
          {!groups.length ? <EmptyState>No accounts yet.</EmptyState> : null}
        </Card>
        <ExpectedActivity data={data} className="account-preview-card account-expected-preview" />
      </div>
    </div>
  )
}

/**
 * A connection Data health says is failing, at the head of the list it affects. Nothing shows
 * while every connection is fine.
 */
function ConnectionNotice({ revision }: { revision: number | undefined }) {
  const navigate = useNavigate()
  const health = useQuery({
    queryKey: ['data-health', revision],
    queryFn: getDataHealth,
    staleTime: 60_000,
    retry: false,
  })
  const issues = (health.data?.issues ?? []).filter(
    ({ category, severity }) => category === 'connection' && severity !== 'info',
  )
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        issues.length
          ? [
              {
                id: 'accounts.connections.review',
                title: 'Review connection issues',
                search: 'connection reconnect provider failed saved data health',
                group: 'Actions',
                scope: 'page',
                run: () => void navigate({ to: '/health' }),
              },
            ]
          : [],
      [issues.length, navigate],
    ),
  )
  const first = issues[0]
  if (!first) return null
  return (
    <div className="accounts-connection-notice" role="status">
      <StatusDot tone={first.severity === 'warning' ? 'warning' : 'negative'} />
      <span>
        <strong>{first.title}</strong>
        <small>
          {first.explanation}
          {issues.length > 1 ? ` · ${countOf(issues.length - 1, 'more issue')}` : ''}
        </small>
      </span>
      <Button
        size="compact"
        variant="secondary"
        data-command="accounts.connections.review"
        onClick={() => void navigate({ to: '/health' })}
      >
        Review
      </Button>
    </div>
  )
}
