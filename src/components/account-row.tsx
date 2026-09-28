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
}: {
  account: Account
  displayName: string
  detail?: string
  valueDetail?: ReactNode
}) {
  return (
    <Link
      className="account-row"
      data-keyboard-row
      data-keyboard-open
      to="/accounts"
      search={{ account: account.id }}
    >
      <AccountMark type={account.type} />
      <span className="account-row-name">
        <strong>{displayName}</strong>
        <small>{account.type}</small>
        {detail ? <small>{detail}</small> : null}
      </span>
      <span className="account-row-values">
        <strong>{formatCurrency(account.value)}</strong>
        {valueDetail}
      </span>
      <ChevronRight size={15} aria-hidden="true" />
    </Link>
  )
}
