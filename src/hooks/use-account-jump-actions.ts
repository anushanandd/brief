import { useNavigate } from '@tanstack/react-router'
import { useMemo } from 'react'

import { accountMenuGroups } from '../components/all-accounts-overview'
import type { AccountGroup } from '../lib/account-groups'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { financeSettings, spendingAccountIds } from '../lib/finance-settings'
import { useLiveFinance } from './use-live-finance'

/**
 * The letter that opens a type of account on Accounts from anywhere. Pages give these letters
 * up: their own single-key actions on B or C take Shift.
 */
const jumpLetters: Record<string, string> = { brokerage: 'b', cash: 'c' }

/** Where a jump lands on Accounts, as its search. */
type AccountsSearch = { group: string } | { account: string }

export type AccountJump = { sequence: [string, string]; title: string; search: AccountsSearch }

/**
 * The jumps in the Accounts menu's order: `b0` is every brokerage, `b1` the first one listed,
 * `b2` the second. A type with one account has no total row, so its `0` opens that account.
 * Digits stop at 9.
 */
export function accountJumps(groups: AccountGroup[], names: Record<string, string>): AccountJump[] {
  return groups.flatMap((group) => {
    const letter = jumpLetters[group.type]
    if (!letter || !group.accounts.length) return []
    const total: AccountJump = {
      sequence: [letter, '0'],
      title: `Open all ${group.label.toLowerCase()} accounts`,
      search: group.accounts.length > 1 ? { group: group.type } : { account: group.accounts[0].id },
    }
    return [
      total,
      ...group.accounts.slice(0, 9).map((account, index) => ({
        sequence: [letter, String(index + 1)] as [string, string],
        title: `Open ${accountDisplayName(account.id, account.name, names)}`,
        search: { account: account.id },
      })),
    ]
  })
}

/** Registers the account jumps; they change only when the menu's order or names do. */
export function useAccountJumpActions() {
  const data = useLiveFinance().data
  const navigate = useNavigate()
  const settings = financeSettings(data)
  const jumps = data
    ? accountJumps(
        accountMenuGroups(data, spendingAccountIds(settings, data.accounts), 0),
        settings.accountNames,
      )
    : []
  // Live prices re-render every tick; the actions follow only the jumps' content.
  const key = JSON.stringify(jumps)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const stableJumps = useMemo(() => jumps, [key])
  const actions = useMemo<AppAction[]>(
    () =>
      stableJumps.map(({ sequence, title, search }) => ({
        id: `navigation.account.${sequence.join('')}`,
        title,
        search: `go account accounts ${title}`,
        group: 'Navigate',
        scope: 'app' as const,
        bindings: [{ sequence, label: sequence.join(' ') }],
        run: () => navigate({ to: '/accounts', search }),
      })),
    [stableJumps, navigate],
  )
  useRegisterActions(actions)
}
