import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useMemo, type ReactNode } from 'react'

import { accountGroups, groupLabel } from '../lib/account-groups'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { getDataHealth } from '../lib/api'
import { financeSettings } from '../lib/finance-settings'
import { countOf } from '../lib/format'
import { chartPeriod } from '../lib/graph-preferences'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { AccountGroupList } from './account-groups'
import { ExpectedActivity } from './expected-activity'
import { Button, Card, EmptyState, SectionHeading, StatusDot } from './ui'

/**
 * Every account at once: the combined chart, any connection that needs attention, each account
 * under its group beside recent activity, and a side column with the combined figures and what's
 * coming up.
 */
export function AllAccountsOverview({
  data,
  accounts,
  spendingCards = [],
  graphWindow,
  chartCard,
  activityPreview,
  details,
}: {
  data: FinanceSnapshot
  accounts: Account[]
  /** Cards Money presents, listed after the rest and left out of the totals. */
  spendingCards?: Account[]
  graphWindow: number
  chartCard: ReactNode
  activityPreview: ReactNode
  /** Investment and income figures across every account. */
  details: ReactNode
}) {
  const names = financeSettings(data).accountNames
  const changeWindow = chartPeriod(graphWindow)
  const periodChanges = data.insights?.changes.find(({ window }) => window === changeWindow)
  // The counted accounts by type, then the spending cards apart, each as Rust totaled them.
  const rollup = data.insights?.rollup
  const groups = [
    ...accountGroups(accounts, rollup?.listed ?? [], periodChanges?.listed),
    ...(rollup?.cards && spendingCards.length
      ? [
          {
            type: 'spending',
            label: groupLabel('spending'),
            accounts: spendingCards.toSorted(
              (left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity),
            ),
            value: rollup.cards.value,
            change: periodChanges?.cards ?? null,
            outsideTotal: true,
          },
        ]
      : []),
  ]

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
