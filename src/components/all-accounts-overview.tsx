import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { useMemo, type ReactNode } from 'react'

import { accountGroups, groupLabel, type AccountGroup } from '../lib/account-groups'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { getDataHealth } from '../lib/api'
import { financeSettings } from '../lib/finance-settings'
import { countOf, formatCurrency } from '../lib/format'
import { chartPeriod } from '../lib/graph-preferences'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { AccountGroupList } from './account-groups'
import { AccountMark } from './account-mark'
import { Button, Card, EmptyState, SectionHeading, StatusDot } from './ui'

/** What the Accounts page shows: every account, one type of account, or one account. */
export type AccountsSelection =
  | { kind: 'all' }
  | { kind: 'group'; group: AccountGroup }
  | { kind: 'account'; account: Account }

/**
 * The counted accounts by type, then the spending cards apart, each as Rust totaled them, with
 * the change over the chart period.
 */
export function accountMenuGroups(
  data: FinanceSnapshot,
  spendingAccountIds: string[],
  graphWindow: number,
): AccountGroup[] {
  const changeWindow = chartPeriod(graphWindow)
  const periodChanges = data.insights?.changes.find(({ window }) => window === changeWindow)
  const rollup = data.insights?.rollup
  const listed = data.accounts.filter(({ id }) => id !== 'all' && !spendingAccountIds.includes(id))
  const spendingCards = data.accounts.filter(({ id }) => spendingAccountIds.includes(id))
  return [
    ...accountGroups(listed, rollup?.listed ?? [], periodChanges?.listed),
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
}

/**
 * Balances as the page's menu: All accounts, then each type's total, then its accounts. Each
 * row changes the view beside it in place; the one shown is marked.
 */
export function AccountsMenu({
  data,
  groups,
  allAccount,
  selection,
  graphWindow,
}: {
  data: FinanceSnapshot
  groups: AccountGroup[]
  allAccount?: Account
  selection: AccountsSelection
  graphWindow: number
}) {
  const names = financeSettings(data).accountNames
  const periodChanges = data.insights?.changes.find(
    ({ window }) => window === chartPeriod(graphWindow),
  )
  return (
    <Card className="account-preview-card all-accounts-card accounts-menu">
      <SectionHeading title="Balances" />
      <nav aria-label="Accounts" data-keyboard-region>
        <Link
          className="account-row accounts-menu-all"
          data-keyboard-row
          data-keyboard-open
          to="/accounts"
          search={{}}
          activeOptions={{ exact: true }}
          aria-current={selection.kind === 'all' ? 'page' : undefined}
        >
          <AccountMark type="combined" />
          <span className="account-row-name">
            <strong>All accounts</strong>
            <small>
              {countOf(data.accounts.filter(({ id }) => id !== 'all').length, 'account')}
            </small>
          </span>
          <span className="account-row-values">
            <strong data-copy-amount>{formatCurrency(allAccount?.value)}</strong>
          </span>
        </Link>
        {groups.length ? (
          <AccountGroupList
            groups={groups}
            changes={periodChanges?.accounts}
            accountNames={names}
            selected={
              selection.kind === 'group'
                ? `group:${selection.group.type}`
                : selection.kind === 'account'
                  ? selection.account.id
                  : undefined
            }
          />
        ) : (
          <EmptyState>No connected accounts.</EmptyState>
        )}
      </nav>
    </Card>
  )
}

/**
 * Every view of Accounts in one layout: the chart over the menu and recent activity, with the
 * view's figures and what's coming up beside them. Choosing in the menu swaps the view, never
 * the page.
 */
export function AccountsLayout({
  revision,
  showConnections,
  chartCard,
  menu,
  activityPreview,
  side,
}: {
  revision: number | undefined
  /** Connection trouble heads the list of every account. */
  showConnections: boolean
  chartCard: ReactNode
  menu: ReactNode
  activityPreview: ReactNode
  side: ReactNode
}) {
  return (
    <div className="account-overview-dashboard all-accounts-overview">
      {/* Columns that stack on their own, so none waits on another's height. */}
      <div className="all-accounts-main">
        {chartCard}
        {showConnections ? <ConnectionNotice revision={revision} /> : null}
        <div className="all-accounts-lists">
          {menu}
          {activityPreview}
        </div>
      </div>
      <div className="all-accounts-side">{side}</div>
    </div>
  )
}

/** The menu's order, for stepping through it with ⌘← and ⌘→. */
export function menuOrder(groups: AccountGroup[]): Array<{ group?: string; account?: string }> {
  return [
    {},
    ...groups.flatMap((group) => [
      ...(group.accounts.length > 1 ? [{ group: group.type }] : []),
      ...group.accounts.map((account) => ({ account: account.id })),
    ]),
  ]
}

/** The name a selection goes by in the breadcrumbs. */
export function selectionLabel(selection: AccountsSelection, names: Record<string, string>) {
  return selection.kind === 'group'
    ? selection.group.label
    : selection.kind === 'account'
      ? accountDisplayName(selection.account.id, selection.account.name, names)
      : 'All accounts'
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
