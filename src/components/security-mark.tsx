import { getExternalLogosEnabled, stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import { BrandMark } from './brand-mark'

/** A security's logo when company logos are enabled under Privacy; its tinted initials otherwise. */
export function SecurityMark({
  ticker,
  name,
  small = false,
}: {
  ticker: string
  name?: string
  small?: boolean
}) {
  return (
    <BrandMark
      className={`asset-mark security-mark${small ? ' security-mark-small' : ''}`}
      fallback={stockMarkLabel(ticker)}
      label={`${name || ticker} logo`}
      src={stockLogoUrl(ticker, getExternalLogosEnabled())}
      style={{ backgroundColor: stockMarkColor(ticker) }}
    />
  )
}
