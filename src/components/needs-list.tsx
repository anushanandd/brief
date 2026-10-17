import { Link } from '@tanstack/react-router'

import { formatCurrency, formatShortDay } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import { needView } from '../lib/needs'
import type { AttentionItem, FinanceSnapshot } from '../lib/schema'
import { AccountMark } from './account-mark'
import { ActivityMark } from './activity-list'
import { recurringActivities } from './expected-activity'
import { BenefitMark } from './platinum-benefits'
import { SecurityMark } from './security-mark'

/** What needs action now, from native insights; nothing renders when nothing does. */
export function NeedsList({ data }: { data: FinanceSnapshot }) {
  const items = data.insights?.brief.needs ?? []
  if (!items.length) return null
  const activities = recurringActivities(
    data,
    items.map((need) => (need.kind === 'possible-duplicate' ? need.transactionIds[1] : undefined)),
  )
  const accountMark = (id: string, fallback = 'credit') => {
    const account = data.accounts.find((candidate) => candidate.id === id)
    return (
      <AccountMark
        type={account?.type ?? fallback}
        institution={account?.institution}
        domain={account?.domain}
      />
    )
  }
  const mark = (need: AttentionItem) => {
    switch (need.kind) {
      case 'possible-duplicate': {
        const activity = activities.get(`spending:${need.transactionIds[1]}`)
        return activity ? <ActivityMark activity={activity} /> : accountMark(need.accountId)
      }
      case 'wash-window':
        return <SecurityMark ticker={need.ticker} />
      case 'credit-expiring':
        return (
          <BenefitMark
            id={need.benefitId}
            name={need.name}
            externalLogosEnabled={getExternalLogosEnabled()}
          />
        )
      case 'coverage-short':
      case 'cash-low':
        return <AccountMark type="cash" />
      case 'contribution-limit':
        return <AccountMark type="retirement" />
      case 'pay-missing':
        return need.accountId ? accountMark(need.accountId) : <AccountMark type="cash" />
      case 'w2-differs':
        return <AccountMark type="cash" />
      case 'missed-bill':
        return need.accountId ? accountMark(need.accountId) : <AccountMark type="cash" />
      case 'program-note':
        return need.subject.startsWith('account:') ? (
          accountMark(need.subject.slice('account:'.length), 'cash')
        ) : (
          <AccountMark type="cash" />
        )
      default:
        return accountMark(need.accountId)
    }
  }
  const needs = items.map((need) => ({ ...needView(need, data), mark: mark(need) }))
  return (
    <section className="coming-up needs-list" aria-labelledby="needs-title">
      <header className="coming-up-heading">
        <h3 id="needs-title">
          Needs you <small>{needs.length}</small>
        </h3>
      </header>
      <ul className="coming-up-list" data-keyboard-region aria-label="Needs you">
        {needs.map((need) => (
          <li key={need.id}>
            <Link
              className={`coming-up-row needs-row${need.urgent ? ' is-urgent' : ''}`}
              data-keyboard-row
              data-keyboard-open
              to={need.link.to}
              search={need.link.search}
            >
              {need.date ? <time dateTime={need.date}>{formatShortDay(need.date)}</time> : <span />}
              {need.mark}
              <span className="coming-up-copy">
                <strong>{need.title}</strong>
                {need.detail ? <small>{need.detail}</small> : null}
              </span>
              {need.amount != null ? (
                <span
                  className={`sensitive-value ${need.amount < 0 ? 'negative' : ''}`}
                  data-copy-amount
                >
                  {formatCurrency(need.amount)}
                </span>
              ) : (
                <span />
              )}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
