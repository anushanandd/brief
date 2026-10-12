import { useParams } from '@tanstack/react-router'

import { InvestmentsWorkspace } from '../components/investments-workspace'

/** Investments: the whole portfolio, or the holding the URL selects. */
export function HoldingsPage() {
  const { ticker } = useParams({ strict: false })
  return <InvestmentsWorkspace ticker={ticker} />
}
