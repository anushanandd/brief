import { Link } from '@tanstack/react-router'
import { useId, useState } from 'react'

import type { AccountGroup } from '../lib/account-groups'
import { accountDisplayName } from '../lib/account-names'
import { countOf, formatCurrency, valueTone } from '../lib/format'
import type { Account } from '../lib/schema'
import { AccountMark } from './account-mark'
import { AccountRow } from './account-row'
import { ChevronDown } from './icons'
import { DisclosedPanel } from './ui/presence'

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
          className="account-row account-group-row tree-parent"
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
        <DisclosedPanel open={open} className="account-group-accounts tree-children" id={panel}>
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
        </DisclosedPanel>
      </div>
    )
  })
}

/**
 * Every account under its group, always open: the group's total and change on its own line,
 * then its accounts. For the Accounts page, where the list is its menu: a group of more than
 * one account opens that group's view, and each account its own.
 */
export function AccountGroupList({
  groups,
  changes = [],
  accountNames,
  selected,
}: {
  groups: AccountGroup[]
  changes?: PeriodChanges
  accountNames: Record<string, string>
  /** The account id, or `group:<type>`, the page is showing. */
  selected?: string
}) {
  return groups.map((group) => {
    // A lone account's balance is the group's, so the heading doesn't repeat it or open a view
    // that would only repeat the account's.
    const single = group.accounts.length === 1
    const heading = (
      <>
        <h3>
          {group.label}
          {group.outsideTotal ? <small> · Not in total</small> : null}
        </h3>
        {single ? null : (
          <>
            {group.change == null ? null : <PeriodChange change={group.change} />}
            <strong className="sensitive-value" data-copy-amount>
              {formatCurrency(group.value)}
            </strong>
          </>
        )}
      </>
    )
    return (
      <section
        key={group.type}
        className="account-group-section"
        aria-label={`${group.label}, ${countOf(group.accounts.length, 'account')}`}
      >
        {single ? (
          <header className="account-group-section-heading">{heading}</header>
        ) : (
          <Link
            className="account-group-section-heading account-group-section-link"
            data-keyboard-row
            data-keyboard-open
            to="/accounts"
            search={{ group: group.type }}
            activeOptions={{ exact: true }}
            aria-current={selected === `group:${group.type}` ? 'page' : undefined}
          >
            {heading}
          </Link>
        )}
        {group.accounts.map((account) => {
          const change = changes.find(({ accountId }) => accountId === account.id)?.change
          return (
            <AccountRow
              key={account.id}
              account={account}
              displayName={accountDisplayName(account.id, account.name, accountNames)}
              detail={
                account.value == null && account.id.startsWith('manual:')
                  ? 'No balance yet; import a statement or enter one'
                  : account.balanceSource === 'spendingOnly'
                    ? 'Tracks your spending; its balance isn’t yours'
                    : account.balanceSource === 'unavailable' || account.value == null
                      ? 'Value unavailable'
                      : account.balanceSource === 'estimated'
                        ? 'Estimated'
                        : undefined
              }
              valueDetail={change == null ? undefined : <PeriodChange change={change} />}
              nested
              current={selected === account.id}
            />
          )
        })}
      </section>
    )
  })
}
