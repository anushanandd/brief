import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { toast } from 'sonner'

import { financeQueryKey, useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { confirmWalletRow, isTauri, syncWallet } from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { financeSettings } from '../../lib/finance-settings'
import { countOf, formatCurrency, formatUpdatedAt } from '../../lib/format'
import type { FinanceSnapshot } from '../../lib/schema'
import { formatActivityDate } from '../../lib/spending'
import { SettingsCard, SettingsRow, SettingsSwitch } from '../settings-ui'
import { Button } from '../ui'

type Wallet = NonNullable<FinanceSnapshot['wallet']>

/** How one card's Wallet records stand against its provider. */
function cardLine(card: Wallet['cards'][number]) {
  const matched = card.matched + card.waiting + card.review
  if (!matched) return `${countOf(card.records, 'purchase')}, from Wallet alone`
  return [
    `${card.matched} matched`,
    card.waiting ? `${card.waiting} waiting for Plaid` : '',
    card.review ? `${card.review} to review` : '',
  ]
    .filter(Boolean)
    .join(' · ')
}

/**
 * Apple Wallet, off until turned on. What it found stays here, where amounts follow screen
 * privacy: how each card compares, Daily Cash against its payouts, and the charges no provider
 * reported, which join history only when confirmed.
 */
export function AppleWalletOption() {
  const finance = useFinance()
  const { updateSettings } = useFinanceEdits()
  const client = useQueryClient()
  const [saving, setSaving] = useState(false)
  const data = finance.data
  const wallet = data?.wallet
  const enabled = Boolean(financeSettings(data).appleWallet)
  const failure = data?.providerStatus?.wallet?.error
  const apply = (view: FinanceSnapshot | null) => {
    if (view) client.setQueryData(financeQueryKey, view)
  }
  const toggle = async (checked: boolean) => {
    setSaving(true)
    try {
      await updateSettings((current) => ({ ...current, appleWallet: checked }))
      apply(await syncWallet())
    } catch (error) {
      toast.error('Could not read Apple Wallet', { description: errorMessage(error) })
    } finally {
      setSaving(false)
    }
  }
  const confirm = (id: string, confirmed: boolean) =>
    void confirmWalletRow(id, confirmed)
      .then(apply)
      .catch((error) =>
        toast.error('Could not save that charge', { description: errorMessage(error) }),
      )
  const cash = wallet?.dailyCash
  const account = wallet?.appleCard
  return (
    <SettingsCard>
      <SettingsRow
        id="apple-wallet"
        title="Apple Wallet"
        description="Adds Apple Card’s history and new card charges as they happen. Read from this Mac; nothing leaves it."
      >
        <SettingsSwitch
          labelledBy="apple-wallet-title"
          describedBy="apple-wallet-description"
          checked={enabled}
          disabled={!isTauri() || saving}
          onCheckedChange={(checked) => void toggle(checked)}
        />
      </SettingsRow>
      {enabled && failure ? (
        <SettingsRow id="apple-wallet-failure" title="Couldn’t read Wallet" description={failure}>
          <Button
            size="compact"
            disabled={saving}
            onClick={() => {
              setSaving(true)
              void syncWallet()
                .then(apply)
                .catch((error) => toast.error(errorMessage(error)))
                .finally(() => setSaving(false))
            }}
          >
            Try again
          </Button>
        </SettingsRow>
      ) : null}
      {wallet?.cards.map((card) => (
        <SettingsRow
          key={card.issuer}
          id={`apple-wallet-card-${card.issuer}`}
          title={card.issuer}
          description={cardLine(card)}
        >
          <small className="settings-section-meta">{`Read ${formatUpdatedAt(wallet.readAt)}`}</small>
        </SettingsRow>
      ))}
      {cash ? (
        <SettingsRow
          id="apple-wallet-daily-cash"
          title="Daily Cash"
          description={
            cash.failingSince
              ? `Payouts have failed since ${formatActivityDate(cash.failingSince)}. Wallet doesn’t show whether Apple credited that cash another way.`
              : 'What Apple Card earned and paid out.'
          }
        >
          <small className="settings-section-meta sensitive-value">
            {`${formatCurrency(cash.earned)} earned · ${formatCurrency(cash.paid)} paid · ${formatCurrency(cash.failed)} failed`}
          </small>
        </SettingsRow>
      ) : null}
      {account ? (
        <SettingsRow
          id="apple-wallet-balance"
          title="Apple Card balance"
          description={
            account.shared
              ? `The whole account, with ${countOf(account.holders, 'holder')}. Someone else pays it, so it isn’t in your net worth or debt.`
              : 'As Apple reports it. It isn’t in your net worth or debt.'
          }
        >
          <small className="settings-section-meta sensitive-value">
            {`${formatCurrency(account.balance)} of ${formatCurrency(account.limit)}`}
          </small>
        </SettingsRow>
      ) : null}
      {wallet?.review.map((row) => (
        <SettingsRow
          key={row.id}
          id={`apple-wallet-review-${row.id}`}
          title={row.merchant}
          description={`${row.account} · ${formatActivityDate(row.date)}. Wallet shows it, Plaid hasn’t reported it.`}
        >
          <span className="settings-section-meta sensitive-value">
            {formatCurrency(row.amount)}
          </span>
          <Button size="compact" onClick={() => confirm(row.id, !row.confirmed)}>
            {row.confirmed ? 'Take out' : 'Add to history'}
          </Button>
        </SettingsRow>
      ))}
      {wallet?.declined.length ? (
        <SettingsRow
          id="apple-wallet-declined"
          title="Declined"
          description={wallet.declined
            .map(
              ({ date, merchant, amount }) =>
                `${merchant}, ${formatCurrency(amount)}, ${formatActivityDate(date)}`,
            )
            .join(' · ')}
        >
          <small className="settings-section-meta">Never counted</small>
        </SettingsRow>
      ) : null}
    </SettingsCard>
  )
}
