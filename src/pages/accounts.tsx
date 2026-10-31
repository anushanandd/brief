import { useNavigate, useRouter, useSearch } from '@tanstack/react-router'
import { useEffect, useMemo } from 'react'

import {
  accountMenuGroups,
  AccountsMenu,
  menuOrder,
  selectionLabel,
  type AccountsSelection,
} from '../components/all-accounts-overview'
import { PageError, PageLoading } from '../components/data-state'
import { WorkspaceHeader } from '../components/workspace-header'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { withoutSpendingCard } from '../lib/dashboard-account-views'
import { financeSettings, spendingAccountIds } from '../lib/finance-settings'
import { useGraphWindow } from '../lib/graph-preferences'
import type { FinanceSnapshot } from '../lib/schema'
import { AccountOverview } from './account-overview'

function useAccountsPage() {
  const query = useLiveFinance()
  if (query.isLoading) return { state: 'loading' as const }
  if (query.isError || !query.data) return { state: 'error' as const }
  return {
    state: 'ready' as const,
    data: withoutSpendingCard(query.data),
    names: financeSettings(query.data).accountNames,
  }
}

export function AccountsPage() {
  const page = useAccountsPage()
  if (page.state === 'loading') return <PageLoading />
  if (page.state === 'error') return <PageError />

  return <AccountsWorkspace data={page.data} names={page.names} />
}

/**
 * Accounts: one layout for every view. Balances is the menu; All accounts is the default, and
 * a type of account or one account replaces the view beside it in place.
 */
function AccountsWorkspace({
  data,
  names,
}: {
  data: FinanceSnapshot
  names: Record<string, string>
}) {
  const routeSearch = useSearch({ from: '/accounts' })
  const spendingIds = spendingAccountIds(financeSettings(data), data.accounts)
  const [graphWindow, setGraphWindow] = useGraphWindow()
  useGraphWindowShortcuts(setGraphWindow)
  // Rust always sends the combined row as `all`.
  const allAccount = data.accounts.find(({ id }) => id === 'all')
  const groups = useMemo(
    () => accountMenuGroups(data, spendingIds, graphWindow),
    // The ids are a fresh array each render; their contents are what matter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data, spendingIds.join(','), graphWindow],
  )
  const group = groups.find(({ type }) => type === routeSearch.group)
  const account = data.accounts.find(({ id }) => id !== 'all' && id === routeSearch.account)
  const selection: AccountsSelection = group
    ? { kind: 'group', group }
    : account
      ? { kind: 'account', account }
      : { kind: 'all' }

  // ⌘← and ⌘→ step through the menu in its order: All accounts, then each type and its accounts.
  const navigate = useNavigate()
  const router = useRouter()
  const order = useMemo(() => menuOrder(groups), [groups])
  const index = Math.max(
    0,
    order.findIndex((item) =>
      selection.kind === 'group'
        ? item.group === selection.group.type
        : selection.kind === 'account'
          ? item.account === selection.account.id
          : !item.group && !item.account,
    ),
  )
  const searchAt = (offset: number) => order[(index + offset + order.length) % order.length]
  useGraphAccountShortcuts((direction) => {
    if (order.length > 1) void navigate({ to: '/accounts', search: searchAt(direction) })
  })
  // The memoized order's entries are stable, so the neighbours change only when the view does.
  const previous = order.length > 1 ? searchAt(-1) : undefined
  const next = order.length > 1 ? searchAt(1) : undefined
  useEffect(() => {
    if (!previous || !next) return undefined
    const idle = globalThis.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 50))
    const cancel = globalThis.cancelIdleCallback ?? clearTimeout
    const handle = idle(() => {
      for (const search of [previous, next])
        void router.preloadRoute({ to: '/accounts', search }).catch(() => undefined)
    })
    return () => cancel(handle)
  }, [previous, next, router])

  const menu = (
    <AccountsMenu
      data={data}
      groups={groups}
      allAccount={allAccount}
      selection={selection}
      graphWindow={graphWindow}
    />
  )
  const shown = selection.kind === 'account' ? selection.account : allAccount

  return (
    <div className="page accounts-workspace-page">
      <WorkspaceHeader
        title="Accounts"
        breadcrumbs={
          selection.kind === 'all'
            ? undefined
            : [
                { label: 'Accounts', to: '/accounts', search: {} },
                {
                  label: selectionLabel(selection, names),
                  to: '/accounts',
                  search:
                    selection.kind === 'group'
                      ? { group: selection.group.type }
                      : { account: selection.account.id },
                },
              ]
        }
      />

      {shown ? (
        // Unkeyed, so the menu stays mounted while the view beside it changes.
        <AccountOverview
          account={shown}
          group={selection.kind === 'group' ? selection.group : undefined}
          menu={menu}
          data={data}
          graphWindow={graphWindow}
          onGraphWindowChange={setGraphWindow}
          spendingAccountIds={spendingIds}
        />
      ) : null}
    </div>
  )
}
