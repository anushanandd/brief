import { getExternalLogosEnabled, institutionLogoUrl } from '../lib/logos'
import { BrandMark } from './brand-mark'
import { ChartNoAxesCombined, CreditCard, Landmark, WalletCards, type IconComponent } from './icons'

const accountMarks: Record<string, IconComponent> = {
  combined: Landmark,
  brokerage: ChartNoAxesCombined,
  retirement: Landmark,
  cash: WalletCards,
  credit: CreditCard,
}

/**
 * An account's institution logo, looked up by its exact domain when company logos are enabled
 * under Privacy; its type icon otherwise or when no logo is found.
 */
export function AccountMark({
  type,
  institution,
  domain,
}: {
  type: string
  institution?: string
  domain?: string
}) {
  const Icon = accountMarks[type] ?? Landmark
  const icon = <Icon size={15} />
  const src = institutionLogoUrl({ institution, domain }, getExternalLogosEnabled())
  return src ? (
    <BrandMark
      className={`transaction-mark account-mark account-mark-${type}`}
      fallback={icon}
      label={`${institution ?? 'Institution'} logo`}
      src={src}
    />
  ) : (
    <span className={`transaction-mark account-mark account-mark-${type}`} aria-hidden="true">
      {icon}
    </span>
  )
}
