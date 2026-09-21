import { useEffect, useMemo, useState } from 'react'

import { localDateKey } from '../lib/analytics'
import { formatUpdatedAt } from '../lib/format'
import { buildHomeSummary, homeSummaryParts } from '../lib/home-summary'
import type { FinanceSnapshot } from '../lib/schema'

export function HomeSummary({
  data,
  rangeSeconds,
  spendingAccountId,
}: {
  data: FinanceSnapshot
  rangeSeconds: number
  spendingAccountId?: string
}) {
  const [today, setToday] = useState(() => localDateKey(Date.now() / 1000))
  useEffect(() => {
    const update = () => setToday(localDateKey(Date.now() / 1000))
    const timer = window.setInterval(update, 60000)
    window.addEventListener('focus', update)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', update)
    }
  }, [])
  const summary = useMemo(
    () => buildHomeSummary(data, rangeSeconds, spendingAccountId, today),
    [data, rangeSeconds, spendingAccountId, today],
  )
  return (
    <section
      className="home-summary"
      aria-label={`Financial summary · Saved ${formatUpdatedAt(data.updatedAt)}`}
    >
      <HomeSummaryText sentences={summary.sentences} evidence={summary.evidence} />
    </section>
  )
}

export function HomeSummaryText({
  sentences,
  evidence,
}: {
  sentences: [string, string]
  evidence: string
}) {
  return (
    <ul>
      {sentences.map((sentence, index) => (
        <li key={index}>
          {homeSummaryParts(sentence, evidence).map((part, partIndex) =>
            part.emphasis ? (
              <strong key={partIndex} className={part.tone}>
                {part.text}
              </strong>
            ) : (
              part.text
            ),
          )}
        </li>
      ))}
    </ul>
  )
}
