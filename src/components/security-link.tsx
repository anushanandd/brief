import { Link } from '@tanstack/react-router'

import { stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import { BrandMark } from './brand-mark'

/** A security's mark and ticker, opening its page on Investments. */
export function SecurityLink({
  ticker,
  name,
  note,
  externalLogosEnabled,
}: {
  ticker?: string | null
  name?: string
  note?: string
  externalLogosEnabled: boolean
}) {
  const label = ticker || 'Security'
  const content = (
    <>
      <BrandMark
        className="asset-mark"
        fallback={stockMarkLabel(label)}
        label={`${name || label} logo`}
        src={ticker ? stockLogoUrl(ticker, externalLogosEnabled) : undefined}
        style={{ backgroundColor: stockMarkColor(label) }}
      />
      <span>
        <strong>{label}</strong>
        {note ? <small>{note}</small> : null}
      </span>
    </>
  )
  return ticker ? (
    <Link
      className="position-security"
      data-keyboard-open
      to="/holdings/$ticker"
      params={{ ticker }}
    >
      {content}
    </Link>
  ) : (
    <span className="position-security">{content}</span>
  )
}
