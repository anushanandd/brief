import { useId, useState } from 'react'

import type { AccountGroup } from '../lib/account-groups'
import { accountDisplayName } from '../lib/account-names'
import { countOf, formatCurrency, valueTone } from '../lib/format'
import type { Account } from '../lib/schema'
import { AccountMark } from './account-mark'
import { AccountRow } from './account-row'
import { ChevronDown } from './icons'

function PeriodChange({ change }: { change: number | null | undefined }) {
  return (
    <small
      className={`sensitive-value ${valueTone(change)}`}
      aria-label={
        change == null
          ? 'No change over the chart period'
          : `Change over the chart period ${formatCurrency(change)}`
      }
    >
      {change == null ? '—' : `${change > 0 ? '+' : ''}${formatCurrency(change)}`}
    </small>
  )
}

type PeriodChanges = Array<{ accountId: string; change: number }>

/** Every account on its own row, with its change over the chart period. */
export function AccountRows({
  accounts,
  changes = [],
  accountNames,
}: {
  accounts: Account[]
  changes?: PeriodChanges
  accountNames: Record<string, string>
}) {
  return accounts.map((account) => (
    <AccountRow
      key={account.id}
      account={account}
      displayName={accountDisplayName(account.id, account.name, accountNames)}
      valueDetail={
        <PeriodChange change={changes.find(({ accountId }) => accountId === account.id)?.change} />
      }
    />
  ))
}

/**
 * Category totals that expand to the accounts behind them. A category of one account is that
 * account's row, since expanding it would only repeat it.
 */
export function AccountGroups({
  groups,
  changes = [],
  accountNames,
}: {
  groups: AccountGroup[]
  changes?: PeriodChanges
  accountNames: Record<string, string>
}) {
  const id = useId()
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set())
  const toggle = (type: string) =>
    setExpanded((current) => {
      const next = new Set(current)
      if (!next.delete(type)) next.add(type)
      return next
    })

  return groups.map((group) => {
    if (group.accounts.length === 1) {
      const [account] = group.accounts
      return (
        <AccountRow
          key={group.type}
          account={account}
          displayName={accountDisplayName(account.id, account.name, accountNames)}
          valueDetail={
            <PeriodChange
              change={changes.find(({ accountId }) => accountId === account.id)?.change}
            />
          }
        />
      )
    }
    const open = expanded.has(group.type)
    const panel = `${id}-${group.type}`
    return (
      <div key={group.type} className="account-group" data-open={open || undefined}>
        <button
          type="button"
          className="account-row account-group-row"
          data-keyboard-row
          data-keyboard-open
          aria-expanded={open}
          aria-controls={panel}
          onClick={() => toggle(group.type)}
        >
          <AccountMark type={group.type} />
          <span className="account-row-name">
            <strong>{group.label}</strong>
            <small>{countOf(group.accounts.length, 'account')}</small>
          </span>
          <span className="account-row-values">
            <strong data-copy-amount>{formatCurrency(group.value)}</strong>
            <PeriodChange change={group.change} />
          </span>
          <ChevronDown size={15} aria-hidden="true" />
        </button>
        {open ? (
          <div className="account-group-accounts" id={panel}>
            {group.accounts.map((account) => (
              <AccountRow
                key={account.id}
                account={account}
                displayName={accountDisplayName(account.id, account.name, accountNames)}
                valueDetail={
                  <PeriodChange
                    change={changes.find(({ accountId }) => accountId === account.id)?.change}
                  />
                }
                nested
              />
            ))}
          </div>
        ) : null}
      </div>
    )
  })
}

/**
 * Every account under its group, always open: the group's total and change on its own line,
 * then its accounts. For the Accounts page, where the list is the way into each account.
 */
export function AccountGroupList({
  groups,
  changes = [],
  accountNames,
}: {
  groups: AccountGroup[]
  changes?: PeriodChanges
  accountNames: Record<string, string>
}) {
  return groups.map((group) => {
    // A lone account's balance is the group's, so the heading doesn't repeat it.
    const single = group.accounts.length === 1
    return (
      <section
        key={group.type}
        className="account-group-section"
        aria-label={`${group.label}, ${countOf(group.accounts.length, 'account')}`}
      >
        <header className="account-group-section-heading">
          <h3>{group.label}</h3>
          {single ? null : (
            <>
              {group.change == null ? null : <PeriodChange change={group.change} />}
              <strong className="sensitive-value" data-copy-amount>
                {formatCurrency(group.value)}
              </strong>
            </>
          )}
        </header>
        {group.accounts.map((account) => {
          const change = changes.find(({ accountId }) => accountId === account.id)?.change
          return (
            <AccountRow
              key={account.id}
              account={account}
              displayName={accountDisplayName(account.id, account.name, accountNames)}
              detail={
                account.balanceSource === 'unavailable' || account.value == null
                  ? 'Value unavailable'
                  : undefined
              }
              valueDetail={change == null ? undefined : <PeriodChange change={change} />}
              nested
            />
          )
        })}
      </section>
    )
  })
}
