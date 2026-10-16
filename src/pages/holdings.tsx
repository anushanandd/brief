import { useLocation, useParams } from '@tanstack/react-router'

import { InvestmentsWorkspace } from '../components/investments-workspace'

/** Investments: the whole portfolio, retirement, or the holding the URL selects. */
export function HoldingsPage() {
  const { ticker } = useParams({ strict: false })
  const retirement = useLocation({ select: ({ pathname }) => pathname === '/holdings/retirement' })
  return <InvestmentsWorkspace ticker={ticker} retirement={retirement} />
}
