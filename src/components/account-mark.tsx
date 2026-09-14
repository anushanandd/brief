import {
  ChartNoAxesCombined,
  CreditCard,
  Landmark,
  WalletCards,
  type LucideIcon,
} from 'lucide-react'

const accountMarks: Record<string, LucideIcon> = {
  combined: Landmark,
  brokerage: ChartNoAxesCombined,
  retirement: Landmark,
  cash: WalletCards,
  credit: CreditCard,
}

export function AccountMark({ type }: { type: string }) {
  const Icon = accountMarks[type] ?? Landmark
  return (
    <span className={`transaction-mark account-mark account-mark-${type}`} aria-hidden="true">
      <Icon size={15} />
    </span>
  )
}
