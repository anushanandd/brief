import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'

import { formatCurrency } from '../lib/format'
import type { Account } from '../lib/schema'
import { AccountMark } from './account-mark'
import { ChevronRight } from './icons'

export function AccountRow({
  account,
  displayName,
  detail,
  valueDetail,
  nested = false,
}: {
  account: Account
  displayName: string
  detail?: string
  valueDetail?: ReactNode
  /** Inside a group that already names the type, so the row names the institution instead. */
  nested?: boolean
}) {
  return (
    <Link
      className={`account-row${nested ? ' account-row-nested' : ''}`}
      data-keyboard-row
      data-keyboard-open
      to="/accounts"
      search={{ account: account.id }}
    >
      <AccountMark type={account.type} institution={account.institution} domain={account.domain} />
      <span className="account-row-name">
        <strong>{displayName}</strong>
        <small>{nested ? account.institution : account.type}</small>
        {detail ? <small>{detail}</small> : null}
      </span>
      <span className="account-row-values">
        <strong data-copy-amount>{formatCurrency(account.value)}</strong>
        {valueDetail}
      </span>
      <ChevronRight size={15} aria-hidden="true" />
    </Link>
  )
}
