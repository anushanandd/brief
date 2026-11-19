import { stockLogoUrl, stockMarkColor, stockMarkLabel, useExternalLogosEnabled } from '../lib/logos'
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
  const externalLogosEnabled = useExternalLogosEnabled()
  return (
    <BrandMark
      className={`asset-mark security-mark${small ? ' security-mark-small' : ''}`}
      fallback={stockMarkLabel(ticker)}
      label={`${name || ticker} logo`}
      src={stockLogoUrl(ticker, externalLogosEnabled)}
      style={{ backgroundColor: stockMarkColor(ticker) }}
    />
  )
}
