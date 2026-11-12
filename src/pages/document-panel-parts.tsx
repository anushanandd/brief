import { Link } from '@tanstack/react-router'
import { useState, type Ref } from 'react'
import { toast } from 'sonner'

import { FilterSelect } from '../components/filter-select'
import type { DocumentChange } from '../lib/api'
import { documentKindNames as kindNames } from '../lib/documents'
import { formatCurrency } from '../lib/format'
import { documentKinds } from '../lib/schema'
import type { BriefDocument, DocumentField, DocumentInsights, FinanceSnapshot } from '../lib/schema'

// The document panel's sections: what the document is (its metadata) and what it says (its
// figures, edited in place), then how its pay reached your accounts.

/**
 * What the document is and whose it is: its kind, tax year, issuer, and account, each edited
 * in place, and its file name. Tucked away once read, open while a statement has no account.
 */
export function DocumentDetails({
  document,
  accounts,
  accountName,
  detailsRef,
  onChange,
}: {
  document: BriefDocument
  accounts: FinanceSnapshot['accounts']
  accountName: (id: string) => string
  /** The disclosure, which the panel's kind and account actions open. */
  detailsRef: Ref<HTMLDetailsElement>
  onChange: (change: DocumentChange) => void
}) {
  return (
    <details
      ref={detailsRef}
      className="documents-details-disclosure"
      open={!document.accountId && (document.kind === 'statement' || document.kind === 'export')}
    >
      <summary>
        Details
        <small>
          {[document.issuer, document.taxYear ? `Tax year ${document.taxYear}` : null]
            .filter(Boolean)
            .join(' · ')}
        </small>
      </summary>
      <div className="documents-details">
        <label>
          <span>Kind</span>
          <FilterSelect
            label="Kind of document"
            command="documents.kind"
            value={document.kind}
            options={Object.entries(kindNames).map(([value, label]) => ({ value, label }))}
            onValueChange={(value) => {
              const kind = documentKinds.find((known) => known === value)
              if (kind) onChange({ kind })
            }}
          />
        </label>
        <label>
          <span>Tax year</span>
          <input
            className="documents-input"
            inputMode="numeric"
            aria-label="Tax year"
            defaultValue={document.taxYear ?? ''}
            onBlur={(event) => {
              const year = Number(event.target.value)
              if (event.target.value && year !== document.taxYear) onChange({ taxYear: year })
            }}
          />
        </label>
        <label>
          <span>From</span>
          <input
            className="documents-input"
            aria-label="Issuer"
            defaultValue={document.issuer ?? ''}
            onBlur={(event) => {
              if (event.target.value !== (document.issuer ?? ''))
                onChange({ issuer: event.target.value })
            }}
          />
        </label>
        <label>
          <span>Account</span>
          <FilterSelect
            label="Account"
            command="documents.account"
            value={document.accountId ?? ''}
            options={[
              { value: '', label: document.mask ? `None · ends ${document.mask}` : 'None' },
              ...accounts
                .filter((account) => account.type !== 'combined')
                .map((account) => ({
                  value: account.id,
                  label: `${accountName(account.id)}${account.mask ? ` ··${account.mask}` : ''}`,
                })),
            ]}
            onValueChange={(accountId) => onChange({ accountId })}
          />
        </label>
      </div>
      <p className="documents-file" title={document.fileName}>
        {document.fileName}
      </p>
    </details>
  )
}

/** What the document says, a figure per row; a pay statement adds its year-to-date column. */
export function DocumentFigures({
  document,
  onChange,
}: {
  document: BriefDocument
  onChange: (change: DocumentChange) => void
}) {
  if (!document.fields.length) return null
  const payslip = document.kind === 'payslip'
  return (
    <section className="documents-block" aria-label="What it says">
      <header className={`documents-field documents-field-head${payslip ? ' has-ytd' : ''}`}>
        <span>Figures</span>
        <span>{payslip ? 'This period' : 'Amount'}</span>
        {payslip ? <span>Year to date</span> : null}
      </header>
      <ul className="documents-fields">
        {document.fields.map((field) => (
          <FieldRow
            key={field.key}
            field={field}
            payslip={payslip}
            onChange={(next) =>
              onChange({
                fields: document.fields.map((item) => (item.key === next.key ? next : item)),
              })
            }
          />
        ))}
      </ul>
    </section>
  )
}

/** One figure, edited in place; a typed value is kept over any later reading. */
function FieldRow({
  field,
  payslip,
  onChange,
}: {
  field: DocumentField
  payslip: boolean
  onChange: (field: DocumentField) => void
}) {
  const [draft, setDraft] = useState<{ amount?: string; ytd?: string }>({})
  const commitValue = (key: 'amount' | 'ytd') => {
    const text = draft[key]
    setDraft((current) => ({ ...current, [key]: undefined }))
    if (text === undefined) return
    const value = text.trim() === '' ? undefined : Number(text.replace(/[$,\s]/g, ''))
    if (value !== undefined && !Number.isFinite(value)) {
      toast.error('Enter an amount')
      return
    }
    onChange({ ...field, [key]: value, edited: true, uncertain: false })
  }
  const doubtful = field.uncertain && !field.edited
  const input = (key: 'amount' | 'ytd', label: string) => (
    <input
      className="documents-amount sensitive-value"
      inputMode="decimal"
      aria-label={`${field.label} ${label}`}
      aria-invalid={doubtful || undefined}
      value={draft[key] ?? (field[key] != null ? field[key].toFixed(2) : '')}
      placeholder="—"
      onChange={(event) => setDraft((current) => ({ ...current, [key]: event.target.value }))}
      onBlur={() => commitValue(key)}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commitValue(key)
      }}
    />
  )
  return (
    <li className={`documents-field${doubtful ? ' is-doubtful' : ''}${payslip ? ' has-ytd' : ''}`}>
      <span className="documents-field-label">
        {field.label}
        {doubtful ? <small title="Compare with the original">Check</small> : null}
        {field.edited ? <small>Yours</small> : null}
      </span>
      {field.text != null ? (
        <span className="documents-field-text">{field.text}</span>
      ) : (
        input('amount', payslip ? 'this period' : 'amount')
      )}
      {payslip ? input('ytd', 'year to date') : null}
    </li>
  )
}

/** Where a pay statement's net pay landed, as Rust matched each deposit to a transaction. */
export function DocumentDeposits({
  pay,
  accountName,
}: {
  pay: DocumentInsights['pay']
  accountName: (id: string) => string
}) {
  if (!pay.length) return null
  return (
    <section className="documents-block" aria-label="Deposits">
      <h3 className="documents-block-title">Deposits</h3>
      <ul className="documents-checks">
        {pay.map((link) => (
          <li
            key={`${link.mask}:${link.amount}`}
            className={link.transactionId ? 'is-done' : link.accountId ? 'is-attention' : ''}
          >
            <span aria-hidden="true">{link.transactionId ? '✓' : link.accountId ? '!' : '·'}</span>
            <span className="sensitive-value">{formatCurrency(link.amount)}</span>
            {link.transactionId ? (
              <Link
                to="/activities"
                search={{ open: link.transactionId }}
                data-command="navigation.open./activities"
              >
                In {link.accountId ? accountName(link.accountId) : 'your account'}
              </Link>
            ) : link.accountId ? (
              `Not yet in ${accountName(link.accountId)}`
            ) : (
              `Untracked account${link.mask ? ` ··${link.mask}` : ''}`
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}

/** A W-2's boxes beside what the year's pay statements predicted for them. */
export function PayPrediction({
  prediction,
}: {
  prediction: DocumentInsights['years'][number]['predictions'][number]
}) {
  return (
    <section className="documents-block" aria-label="Against your pay statements">
      <header className="documents-field documents-field-head has-ytd">
        <span>Against pay statements</span>
        <span>Pay</span>
        <span>W-2</span>
      </header>
      <ul className="documents-fields">
        {prediction.boxes.map((item) => {
          const differs = item.actual != null && Math.abs(item.actual - item.predicted) > 1
          return (
            <li
              key={item.key}
              className={`documents-field has-ytd${differs ? ' is-doubtful' : ''}`}
            >
              <span className="documents-field-label">{item.label}</span>
              <span className="documents-field-text sensitive-value">
                {formatCurrency(item.predicted)}
              </span>
              <span className="documents-field-text sensitive-value">
                {item.actual == null
                  ? 'Not read'
                  : differs
                    ? formatCurrency(item.actual)
                    : 'Matches'}
              </span>
            </li>
          )
        })}
      </ul>
    </section>
  )
}
