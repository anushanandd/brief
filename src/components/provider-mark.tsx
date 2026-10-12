import { getExternalLogosEnabled, brandLogoUrl } from '../lib/logos'
import { BrandMark } from './brand-mark'
import {
  Building,
  CalendarDays,
  ChartCandlestick,
  ChartNoAxesCombined,
  Landmark,
  type IconComponent,
} from './icons'

export type ProviderId = 'plaid' | 'snaptrade' | 'alpaca' | 'alphavantage' | 'sec'

const providerMarks: Record<ProviderId, { domain: string; Icon: IconComponent; color: string }> = {
  plaid: { domain: 'plaid.com', Icon: Landmark, color: 'var(--data-1)' },
  snaptrade: { domain: 'snaptrade.com', Icon: ChartNoAxesCombined, color: 'var(--data-3)' },
  alpaca: { domain: 'alpaca.markets', Icon: ChartCandlestick, color: 'var(--data-2)' },
  alphavantage: { domain: 'alphavantage.co', Icon: CalendarDays, color: 'var(--data-4)' },
  sec: { domain: 'sec.gov', Icon: Building, color: 'var(--data-5)' },
}

/** A provider's logo when company logos are enabled under Privacy; a tinted icon otherwise. */
export function ProviderMark({ provider, name }: { provider: ProviderId; name: string }) {
  const { domain, Icon, color } = providerMarks[provider]
  return (
    <BrandMark
      className="transaction-mark provider-mark"
      style={{
        color,
        backgroundColor: `color-mix(in srgb, ${color} 14%, var(--surface-raised))`,
      }}
      label={`${name} logo`}
      src={getExternalLogosEnabled() ? brandLogoUrl(name, domain, true) : undefined}
      fallback={<Icon size={15} />}
    />
  )
}
