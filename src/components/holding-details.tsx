import { Link } from '@tanstack/react-router'
import { useId, useMemo, useRef, useState } from 'react'

import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { financeSettings } from '../lib/finance-settings'
import {
  formatCurrency,
  formatPercent,
  formatPublicCurrency,
  formatRate,
  formatShare,
  formatShares,
  formatSignedCurrency,
  valueTone,
  formatDay,
} from '../lib/format'
import { holdingDetail } from '../lib/holding-detail'
import type { FinanceSnapshot } from '../lib/schema'
import { AccountMark } from './account-mark'
import { ActivityList } from './activity-list'
import { HoldingEarnings } from './holding-earnings'
import { ChevronRight, ReceiptText, X } from './icons'
import { useObjectDetailsAction } from './object-details'
import { SecurityMark } from './security-mark'
import { SettingsLink } from './settings-dialog'
import { Button, Card, EmptyState, Metric, ScrollCueCard, SectionHeading } from './ui'
import { Dialog, dialogAutofocus } from './ui/dialog'

/**
 * What you own of one security, in a card beside its price: worth and weight, gain, shares,
 * and cost. Taxes matter when deciding to sell, not on every look, so they open on request.
 */
/** Pooled instruments, which have no earnings dates of their own. */
export const isFundKind = (kind: string | null | undefined) =>
  /^(etf|mutual ?_?fund|fund|cef|index)$/i.test(kind?.trim() ?? '')

export function PositionCard({
  data,
  ticker,
  earningsConnected,
}: {
  data: FinanceSnapshot
  ticker: string
  earningsConnected: boolean
}) {
  const detail = holdingDetail(data, ticker)
  const security = data.insights?.securities.find((item) => item.ticker === ticker)
  const lots = (data.insights?.lots ?? []).filter((position) => position.ticker === ticker)
  const held = detail.positions
  const dividends = data.insights?.dividends.securities.find(
    (item) => item.ticker.toUpperCase() === ticker.toUpperCase(),
  )
  const vested = detail.vestedShares
  const bond = held.find(({ fixedIncome }) => fixedIncome)?.fixedIncome
  const option = held.find((holding) => holding.option)?.option
  const described = held.find(
    ({ sector, industry, identifiers }) => sector || industry || identifiers,
  )
  const descriptors = [
    described?.sector,
    described?.industry,
    described?.identifiers?.cusip ? `CUSIP ${described.identifiers.cusip}` : undefined,
    described?.identifiers?.isin && !described.identifiers.cusip
      ? `ISIN ${described.identifiers.isin}`
      : undefined,
    described?.identifiers?.exchange,
  ].filter(Boolean)
  // A money-weighted return is per account; with several accounts there is no single figure.
  const moneyWeighted = lots.length === 1 ? lots[0] : undefined
  const [taxesOpen, setTaxesOpen] = useState(false)
  const taxActions = useMemo<AppAction[]>(
    () => [
      {
        id: 'holding.taxes',
        title: 'Show taxes',
        search: 'holding taxes lots long-term short-term gain',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['t'], label: 'T' }],
        run: () => setTaxesOpen(true),
      },
    ],
    [],
  )
  useRegisterActions(taxActions)
  // ⌘I shows the first account's position; each account's is its own object.
  useObjectDetailsAction(held[0] ? `holding:${held[0].accountId}:${ticker}` : undefined)
  return (
    <Card className="investments-side-card position-card" aria-label={`Your ${ticker} position`}>
      <SectionHeading title="Your position" />
      <div className="investments-side-stats">
        <Metric
          label="Value"
          value={
            <>
              <span className="sensitive-value">{formatCurrency(detail.value)}</span>
              {security?.percent != null ? (
                <small className="metric-detail">{`${formatShare(security.percent)} of portfolio`}</small>
              ) : null}
            </>
          }
        />
        <Metric
          label="Gain"
          value={
            <>
              <span className="sensitive-value">
                {detail.gain == null
                  ? '—'
                  : `${detail.gain >= 0 ? '+' : ''}${formatCurrency(detail.gain)}`}
              </span>
              {detail.totalChange != null ? (
                <small className="metric-detail">{formatPercent(detail.totalChange)}</small>
              ) : null}
            </>
          }
          tone={valueTone(detail.gain)}
        />
        {moneyWeighted?.moneyWeightedPct != null ? (
          <Metric
            label="Money-weighted"
            value={formatPercent(moneyWeighted.moneyWeightedPct)}
            tone={valueTone(moneyWeighted.moneyWeightedPct)}
          />
        ) : null}
        <Metric
          label="Shares"
          value={
            <>
              {formatShares(detail.shares)}
              {vested != null ? (
                <small className="metric-detail">{`${formatShares(vested)} vested`}</small>
              ) : null}
            </>
          }
        />
        <Metric
          label="Average cost"
          value={<span className="sensitive-value">{formatCurrency(detail.averageBasis)}</span>}
        />
        {dividends ? (
          <Metric
            label="Dividend income"
            value={
              <>
                <span className="sensitive-value">{formatCurrency(dividends.annualIncome)}</span>
                <small className="metric-detail">
                  {dividends.yieldPercent != null
                    ? `/yr · ${formatRate(dividends.yieldPercent)} yield`
                    : '/yr'}
                </small>
              </>
            }
          />
        ) : null}
        {dividends?.next ? (
          <Metric
            label={dividends.next.special ? 'Special dividend' : 'Next dividend'}
            value={
              <>
                <span className="sensitive-value">{formatCurrency(dividends.next.amount)}</span>
                <small className="metric-detail">
                  {dividends.next.payableDate
                    ? `pays ${formatDay(dividends.next.payableDate)}`
                    : `ex ${formatDay(dividends.next.exDate)}`}
                </small>
              </>
            }
          />
        ) : null}
        {bond ? (
          <Metric
            label="Yield"
            value={
              <>
                {bond.yieldPercent != null ? formatRate(bond.yieldPercent) : '—'}
                {bond.maturityDate ? (
                  <small className="metric-detail">{`matures ${formatDay(bond.maturityDate)}`}</small>
                ) : null}
              </>
            }
          />
        ) : null}
        {option ? (
          <Metric
            label={option.contractType === 'put' ? 'Put' : 'Call'}
            value={
              <>
                {option.strikePrice != null ? formatPublicCurrency(option.strikePrice) : '—'}
                {option.expirationDate ? (
                  <small className="metric-detail">{`expires ${formatDay(option.expirationDate)}`}</small>
                ) : null}
              </>
            }
          />
        ) : null}
        {/* Funds report no earnings, so the row is left out rather than shown as unavailable. */}
        {held.some((holding) => isFundKind(holding.instrumentKind)) ? null : (
          <HoldingEarnings symbols={ticker ? [ticker] : []} connected={earningsConnected} />
        )}
      </div>
      {descriptors.length ? <p className="portfolio-note">{descriptors.join(' · ')}</p> : null}
      <PositionAccounts data={data} ticker={ticker} />
      <div className="position-card-action">
        <Button
          type="button"
          icon={ReceiptText}
          aria-haspopup="dialog"
          aria-expanded={taxesOpen}
          aria-keyshortcuts="T"
          data-command="holding.taxes"
          onClick={() => setTaxesOpen(true)}
        >
          Taxes
        </Button>
      </div>
      <SecurityTaxesDialog
        data={data}
        ticker={ticker}
        open={taxesOpen}
        onClose={() => setTaxesOpen(false)}
      />
    </Card>
  )
}

/**
 * The position's parts, one per account, behind a disclosure: shown only when the security is
 * held in more than one account, since one account's part is the whole position above.
 */
function PositionAccounts({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const detailsRef = useRef<HTMLDetailsElement>(null)
  const parts = holdingDetail(data, ticker).accounts
  const several = parts.length > 1
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        several
          ? [
              {
                id: 'holding.accounts.toggle',
                title: 'Show the position by account',
                search: 'holding position accounts split breakdown',
                group: 'View',
                scope: 'page',
                bindings: [{ sequence: ['i'], label: 'I' }],
                run: () => {
                  const details = detailsRef.current
                  if (details) details.open = !details.open
                },
              },
            ]
          : [],
      [several],
    ),
  )
  if (!several) return null
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(id, data.accounts.find((account) => account.id === id)?.name ?? id, names)
  return (
    <details ref={detailsRef} className="account-details-more position-accounts">
      <summary data-command="holding.accounts.toggle">{`In ${parts.length} accounts`}</summary>
      <table className="financial-table" aria-label={`Your ${ticker} position by account`}>
        <thead>
          <tr>
            <th scope="col">Account</th>
            <th scope="col">Shares</th>
            <th scope="col">Value</th>
            <th scope="col">Cost basis</th>
            <th scope="col">Gain</th>
          </tr>
        </thead>
        <tbody>
          {parts.map((part) => (
            <tr key={part.accountId}>
              <th scope="row">{accountName(part.accountId)}</th>
              <td>{formatShares(part.shares)}</td>
              <td className="sensitive-value">{formatCurrency(part.value)}</td>
              <td className="sensitive-value">{formatCurrency(part.costBasis)}</td>
              <td className={`sensitive-value ${valueTone(part.unrealizedGain)}`}>
                {formatCurrency(part.unrealizedGain)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </details>
  )
}

/** Where one security is held, each account with its part of the position. */
export function HoldingAccounts({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const detail = holdingDetail(data, ticker)
  const names = financeSettings(data).accountNames
  const accountIds = detail.accounts.map(({ accountId }) => accountId)
  return (
    <Card className="holding-detail-card holding-accounts-card">
      <SectionHeading title="Held in" />
      {accountIds.length ? (
        <div
          className="security-position-accounts"
          aria-label={`Accounts holding ${ticker}`}
          data-keyboard-region
        >
          {accountIds.map((id) => {
            const account = data.accounts.find((candidate) => candidate.id === id)
            const position = detail.accounts.find(({ accountId }) => accountId === id)
            return (
              <Link
                key={id}
                className="account-row"
                to="/accounts"
                search={{ account: id }}
                data-keyboard-row
                data-keyboard-open
              >
                <AccountMark
                  type={account?.type ?? 'brokerage'}
                  institution={account?.institution}
                  domain={account?.domain}
                />
                <span className="account-row-name">
                  <strong>{accountDisplayName(id, account?.name ?? 'Account', names)}</strong>
                  <small>{account?.institution}</small>
                </span>
                <span className="account-row-values">
                  <strong className="sensitive-value" data-copy-amount>
                    {formatCurrency(position?.value)}
                  </strong>
                  <small>{formatShares(position?.shares)} shares</small>
                </span>
                <ChevronRight size={15} aria-hidden="true" />
              </Link>
            )
          })}
        </div>
      ) : (
        <EmptyState>No positions available.</EmptyState>
      )}
    </Card>
  )
}

const signedAmount = formatSignedCurrency

/** Taxes on one security, docked to the right edge like the transaction inspector. */
function SecurityTaxesDialog({
  data,
  ticker,
  open,
  onClose,
}: {
  data: FinanceSnapshot
  ticker: string
  open: boolean
  onClose: () => void
}) {
  const titleId = useId()
  return (
    <Dialog
      open={open}
      onClose={onClose}
      popups
      className="transaction-dialog security-taxes-dialog"
      aria-labelledby={titleId}
    >
      {open ? (
        <article className="transaction-details">
          <header className="transaction-details-header">
            <SecurityMark ticker={ticker} />
            <div>
              {/* The title takes focus on open, so no control starts out ringed. */}
              <h2 id={titleId} tabIndex={-1} {...dialogAutofocus}>
                Taxes
              </h2>
              <p>{ticker}</p>
            </div>
            <Button
              icon={X}
              size="icon-compact"
              variant="ghost"
              aria-label="Close taxes"
              onClick={onClose}
            />
          </header>
          <div className="security-taxes-body">
            <SecurityTaxes data={data} ticker={ticker} />
          </div>
        </article>
      ) : null}
    </Dialog>
  )
}

/** Taxes on one security: what a sale would realize, the next long-term date, and the lots. */
export function SecurityTaxes({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const positions = (data.insights?.lots ?? []).filter((position) => position.ticker === ticker)
  const soon = (data.insights?.longTermSoon ?? []).filter((lot) => lot.ticker === ticker)
  const taxes = data.insights?.taxes
  const withTax = positions.some(({ lots }) => lots.some((lot) => lot.estimatedTax != null))
  const windows = (taxes?.washWindows ?? []).filter((window) => window.ticker === ticker)
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(id, data.accounts.find((account) => account.id === id)?.name ?? id, names)
  return (
    <>
      {windows.map((window) => (
        <p key={window.saleId} className="security-taxes-flag">
          <span className="sensitive-value">{formatCurrency(Math.abs(window.loss))}</span> loss on{' '}
          {formatDay(window.soldOn)}. Don’t buy {ticker} in any account through{' '}
          {formatDay(window.until)}.
        </p>
      ))}
      {!positions.length ? (
        <EmptyState>Needs this holding’s purchases in your activity.</EmptyState>
      ) : (
        positions.map((position) => {
          const next = position.lots
            .filter((lot) => !lot.longTerm)
            .map((lot) => lot.longTermOn)
            .toSorted()[0]
          const upcoming = soon.find((lot) => lot.accountId === position.accountId)
          const harvest = taxes?.harvest.find(
            (item) => item.ticker === ticker && item.accountId === position.accountId,
          )
          return (
            <section
              key={position.accountId}
              className="security-taxes"
              aria-label={`Taxes in ${accountName(position.accountId)}`}
            >
              {positions.length > 1 ? <h3>{accountName(position.accountId)}</h3> : null}
              {position.taxDeferred ? (
                <p>Retirement account: no tax on sales inside it.</p>
              ) : (
                <>
                  <div className="account-summary-metrics">
                    <Metric
                      label="Short-term gain"
                      value={
                        <span className="sensitive-value">
                          {signedAmount(position.shortTermGain)}
                        </span>
                      }
                      tone={valueTone(position.shortTermGain)}
                    />
                    <Metric
                      label="Long-term gain"
                      value={
                        <span className="sensitive-value">
                          {signedAmount(position.longTermGain)}
                        </span>
                      }
                      tone={valueTone(position.longTermGain)}
                    />
                    <Metric
                      label="Next long-term"
                      value={
                        upcoming ? (
                          <>
                            {formatDay(upcoming.date)}
                            <small className="metric-detail">
                              {`${formatShares(upcoming.units)} sh`}
                            </small>
                          </>
                        ) : next ? (
                          formatDay(next)
                        ) : (
                          'All long-term'
                        )
                      }
                    />
                  </div>
                  {upcoming?.gain != null && upcoming.gain > 0 ? (
                    <p>
                      Wait until {formatDay(upcoming.date)} to avoid short-term tax on{' '}
                      <span className="sensitive-value">{formatCurrency(upcoming.gain)}</span>.
                    </p>
                  ) : null}
                  {harvest ? (
                    <p>
                      Selling now harvests a loss
                      {harvest.taxSavings != null ? (
                        <>
                          , saving ~
                          <span className="sensitive-value">
                            {formatCurrency(harvest.taxSavings)}
                          </span>
                        </>
                      ) : null}
                      .{harvest.washUntil ? ` Sell after ${formatDay(harvest.washUntil)}.` : ''}
                    </p>
                  ) : null}
                </>
              )}
              <table className="financial-table portfolio-returns security-lots">
                <thead>
                  <tr>
                    <th scope="col">Acquired</th>
                    <th scope="col">Shares</th>
                    <th scope="col">Cost</th>
                    <th scope="col">Gain</th>
                    {withTax ? <th scope="col">Tax if sold</th> : null}
                    <th scope="col">Term</th>
                  </tr>
                </thead>
                <tbody>
                  {position.lots.map((lot) => (
                    <tr key={`${lot.acquired}:${lot.units}:${lot.cost}`}>
                      <th scope="row">{formatDay(lot.acquired)}</th>
                      <td className="sensitive-value">{formatShares(lot.units)}</td>
                      <td className="sensitive-value">{formatCurrency(lot.cost)}</td>
                      <td className={`sensitive-value ${valueTone(lot.gain)}`}>
                        {lot.gain == null ? 'Unavailable' : formatCurrency(lot.gain)}
                      </td>
                      {withTax ? (
                        <td className="sensitive-value">
                          {lot.estimatedTax == null ? '—' : formatCurrency(lot.estimatedTax)}
                        </td>
                      ) : null}
                      <td>{lot.longTerm ? 'Long' : `Short · long ${formatDay(lot.longTermOn)}`}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {position.coverage === 'partial' ? (
                <small>{`${formatShares(position.untrackedUnits)} shares untracked; figures cover the rest.`}</small>
              ) : null}
            </section>
          )
        })
      )}
      <p className="portfolio-note">
        FIFO estimate; your broker may differ.{' '}
        {taxes?.hasProfile ? null : (
          <SettingsLink target="tax-filing-status">Add tax profile</SettingsLink>
        )}{' '}
        <Link to="/taxes" data-command="navigation.open./taxes">
          All taxes
        </Link>
      </p>
    </>
  )
}

export function HoldingActivity({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const activityScrollRef = useViewportScroll(48)
  const activities = buildActivities(
    {
      transactions: [],
      trades: data.trades.filter((trade) => trade.ticker === ticker),
      updatedAt: data.updatedAt,
    },
    financeSettings(data).accountNames,
  )
  return (
    <ScrollCueCard className="holding-detail-card" scrollSelector=".holding-activity-scroll">
      <SectionHeading title="Activity" />
      <div
        ref={activityScrollRef}
        className="holding-activity-scroll"
        role="region"
        aria-label={`${ticker} activity`}
        tabIndex={0}
      >
        <ActivityList
          activities={activities}
          referenceIso={data.updatedAt}
          emptyMessage={`No imported activity linked to ${ticker || 'this holding'}.`}
        />
      </div>
    </ScrollCueCard>
  )
}
