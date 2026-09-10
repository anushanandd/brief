import { WalletCards } from 'lucide-react'

import { formatCurrency } from '../lib/format'
import {
  getExternalLogosEnabled,
  transactionLogoUrl,
  transactionMarkKind,
  transactionMarkLabel,
} from '../lib/logos'
import type { Transaction } from '../lib/schema'
import { formatTransactionDate, transactionDateKey } from '../lib/spending'
import { BrandMark } from './brand-mark'

export function TransactionList({
  transactions,
  referenceIso,
  detailed = false,
  onBenefitConfirmation,
  savingConfirmation = false,
}: {
  transactions: Transaction[]
  referenceIso: string
  detailed?: boolean
  onBenefitConfirmation?: (id: string, confirmed: boolean) => void
  savingConfirmation?: boolean
}) {
  const externalLogosEnabled = getExternalLogosEnabled()
  return (
    <div className={`spending-transaction-list${detailed ? ' transaction-history-list' : ''}`}>
      {transactions.map((transaction) => (
        <div className="spending-transaction-row" key={transaction.id}>
          <BrandMark
            className={`transaction-mark transaction-mark-${transactionMarkKind(transaction)}`}
            fallback={transactionMarkLabel(transaction)}
            label={`${transaction.merchant} transaction icon`}
            src={transactionLogoUrl(transaction, externalLogosEnabled)}
          />
          <span className="transaction-name">
            <strong>{transaction.merchant}</strong>
            <small>
              {detailed
                ? transactionDateKey(transaction.date, referenceIso) || transaction.date
                : formatTransactionDate(transaction.date, referenceIso)}
              {transaction.pending ? ' · Pending' : ''}
            </small>
            {detailed &&
            transaction.description &&
            transaction.description !== transaction.merchant ? (
              <small className="transaction-description">{transaction.description}</small>
            ) : null}
            {onBenefitConfirmation && transaction.amount > 0 && !transaction.pending ? (
              <label className="benefit-confirmation">
                <input
                  type="checkbox"
                  checked={transaction.benefitConfirmed === true}
                  disabled={savingConfirmation}
                  onChange={(event) => onBenefitConfirmation(transaction.id, event.target.checked)}
                />{' '}
                Confirmed benefit reimbursement
              </label>
            ) : null}
          </span>
          <span className="transaction-category" title={transaction.category}>
            {transaction.category}
          </span>
          <strong className={transaction.amount > 0 ? 'positive' : undefined}>
            {formatCurrency(transaction.amount)}
          </strong>
        </div>
      ))}
      {!transactions.length ? (
        <div className="spending-empty">
          <WalletCards size={18} aria-hidden="true" />
          <span>No matching transactions.</span>
        </div>
      ) : null}
    </div>
  )
}
