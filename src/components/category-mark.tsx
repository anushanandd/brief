import type { ActivityItem } from '../lib/activity'
import { spendingCategoryColor } from '../lib/spending'
import {
  ArrowLeftRight,
  BadgeDollarSign,
  Banknote,
  BedDouble,
  Building,
  CarFront,
  ChartCandlestick,
  Clapperboard,
  Coffee,
  Coins,
  CreditCard,
  Fuel,
  Gift,
  GraduationCap,
  HandCoins,
  HeartPulse,
  House,
  Invoice,
  Landmark,
  MoneySend,
  Package,
  Percent,
  Plane,
  ReceiptText,
  RotateCcw,
  Shapes,
  ShieldCheck,
  ShoppingBag,
  ShoppingBasket,
  Sofa,
  Sparkles,
  Tag,
  UtensilsCrossed,
  Wrench,
  Zap,
  type IconComponent,
} from './icons'

type Mark = { icon: IconComponent; color: string }

const transactionMarks: Record<NonNullable<ActivityItem['mark']>, Mark> = {
  transfer: { icon: ArrowLeftRight, color: 'var(--chart-secondary)' },
  interest: { icon: Percent, color: 'var(--positive)' },
  dividend: { icon: Coins, color: 'var(--positive)' },
  income: { icon: HandCoins, color: 'var(--positive)' },
  refund: { icon: RotateCcw, color: 'var(--positive)' },
  fee: { icon: ReceiptText, color: 'var(--negative)' },
  payment: { icon: CreditCard, color: 'var(--chart-secondary)' },
  cash: { icon: Banknote, color: 'var(--chart-secondary)' },
  initial: { icon: Tag, color: 'var(--spending-other)' },
}

const namedMarks: Record<string, Mark> = {
  income: transactionMarks.income,
  credit: { icon: BadgeDollarSign, color: 'var(--positive)' },
  trade: { icon: ChartCandlestick, color: 'var(--chart-benchmark)' },
  transfer: transactionMarks.transfer,
  'transfer in': transactionMarks.transfer,
  'transfer out': transactionMarks.transfer,
  interest: transactionMarks.interest,
  dividend: transactionMarks.dividend,
  dividends: transactionMarks.dividend,
  refund: transactionMarks.refund,
  payment: transactionMarks.payment,
  cash: transactionMarks.cash,
  'bank fees': transactionMarks.fee,
  other: { icon: Shapes, color: 'var(--spending-other)' },
  dining: { icon: UtensilsCrossed, color: 'var(--spending-food)' },
  groceries: { icon: ShoppingBasket, color: 'var(--spending-food)' },
  'food and drink': { icon: Coffee, color: 'var(--spending-food)' },
  shopping: { icon: ShoppingBag, color: 'var(--spending-shopping)' },
  transport: { icon: CarFront, color: 'var(--spending-transport)' },
  travel: { icon: Plane, color: 'var(--spending-travel)' },
  utilities: { icon: Zap, color: 'var(--spending-utilities)' },
  entertainment: { icon: Clapperboard, color: 'var(--spending-entertainment)' },
  // Brief's top-level groups for Plaid's categories (src-tauri/src/financial_engine/categories.json).
  rent: { icon: House, color: 'var(--spending-housing)' },
  home: { icon: Sofa, color: 'var(--spending-housing)' },
  health: { icon: HeartPulse, color: 'var(--data-4)' },
  'personal care': { icon: Sparkles, color: 'var(--data-5)' },
  services: { icon: Wrench, color: 'var(--data-2)' },
  giving: { icon: Gift, color: 'var(--data-1)' },
  government: { icon: Building, color: 'var(--data-6)' },
  taxes: { icon: Invoice, color: 'var(--data-3)' },
  fees: transactionMarks.fee,
  transfers: transactionMarks.transfer,
  'loan payments': transactionMarks.payment,
  'payments to people': { icon: MoneySend, color: 'var(--data-1)' },
}

const categoryPatterns: { test: RegExp; icon: IconComponent; color?: string }[] = [
  { test: /grocer|supermarket/, icon: ShoppingBasket },
  { test: /dining|restaurant/, icon: UtensilsCrossed },
  { test: /coffee|cafe|food|drink/, icon: Coffee },
  { test: /hotel|lodging/, icon: BedDouble },
  { test: /travel|flight|airline/, icon: Plane },
  { test: /fuel|gas station/, icon: Fuel },
  { test: /transport|taxi|rideshare|parking/, icon: CarFront },
  { test: /rent|mortgage|housing|home/, icon: House },
  { test: /retail|shopping/, icon: ShoppingBag },
  { test: /merchandise|goods/, icon: Package },
  { test: /movie|film|entertainment|subscription/, icon: Clapperboard },
  { test: /utilit|electric|water|internet|phone|bill/, icon: Zap },
  { test: /medical|health|pharmacy/, icon: HeartPulse, color: 'var(--data-4)' },
  { test: /education|tuition|school/, icon: GraduationCap, color: 'var(--data-1)' },
  { test: /insurance/, icon: ShieldCheck, color: 'var(--data-6)' },
  { test: /tax/, icon: Landmark, color: 'var(--data-3)' },
  { test: /personal care|beauty/, icon: Sparkles, color: 'var(--data-5)' },
  { test: /service|repair/, icon: Wrench, color: 'var(--data-2)' },
]

type MarkInput = {
  category: string
  kind?: ActivityItem['kind']
  mark?: ActivityItem['mark']
  amount?: number
}

/** The color an activity's category mark wears, for anything that labels it elsewhere. */
export const categoryColor = (input: MarkInput) => categoryMark(input).color

function categoryMark({ category, kind, mark, amount }: MarkInput) {
  const name = category.trim().toLocaleLowerCase()
  const selected =
    kind === 'transfer' || mark === 'transfer'
      ? transactionMarks.transfer
      : kind === 'trade'
        ? namedMarks.trade
        : kind === 'credit'
          ? namedMarks.credit
          : mark && mark !== 'initial'
            ? transactionMarks[mark]
            : namedMarks[name]
  const inferred = selected ? undefined : categoryPatterns.find(({ test }) => test.test(name))
  const color =
    kind === 'transfer' || mark === 'transfer'
      ? amount == null || amount === 0
        ? 'var(--chart-secondary)'
        : amount > 0
          ? 'var(--positive)'
          : 'var(--negative)'
      : (selected?.color ?? inferred?.color ?? spendingCategoryColor(category))
  return { color, icon: selected?.icon ?? inferred?.icon ?? Tag }
}

export function CategoryMark({ className, ...input }: MarkInput & { className?: string }) {
  const { color, icon: Icon } = categoryMark(input)
  return (
    <span
      className={`transaction-mark category-mark${className ? ` ${className}` : ''}`}
      style={{ color, backgroundColor: `color-mix(in srgb, ${color} 14%, var(--surface-raised))` }}
      aria-hidden="true"
    >
      <Icon size={15} />
    </span>
  )
}
