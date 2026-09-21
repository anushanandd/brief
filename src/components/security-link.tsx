import { stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import { BrandMark } from './brand-mark'
import { ExternalLink } from './external-link'

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
    <ExternalLink
      className="position-security"
      data-keyboard-open
      href={`https://finance.yahoo.com/quote/${encodeURIComponent(ticker)}/`}
    >
      {content}
    </ExternalLink>
  ) : (
    <span className="position-security">{content}</span>
  )
}
