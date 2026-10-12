import { useQuery } from '@tanstack/react-query'

import { useFinance } from '../hooks/use-finance'
import { isTauri, ledgerQuery } from '../lib/api'
import { ledgerView } from '../lib/ledger-view'
import { savePins, usePins, type Pin } from '../lib/pins'
import { X } from './icons'
import { LedgerResult } from './ledger-result'
import { Button, Card, SectionHeading } from './ui'

function PinnedCard({ pin, onRemove }: { pin: Pin; onRemove: () => void }) {
  const revision = useFinance().data?.revision
  const native = typeof window !== 'undefined' && isTauri()
  // Recomputed from saved data whenever it changes; the language model is not involved.
  const result = useQuery({
    queryKey: ['ledger-query', pin.name, pin.arguments, revision],
    queryFn: () => ledgerQuery(pin.name, pin.arguments),
    enabled: native,
    retry: false,
  })
  const view = ledgerView({ name: pin.name, arguments: pin.arguments, result: result.data ?? {} })
  return (
    <Card className="pinned-card">
      <SectionHeading
        title={view.title}
        action={
          <Button
            icon={X}
            size="icon-compact"
            variant="ghost"
            aria-label={`Remove ${view.title} from Today`}
            onClick={onRemove}
          />
        }
      />
      <p className="pinned-question">
        {pin.question}
        {view.scope ? ` · ${view.scope}` : ''}
      </p>
      {!native ? (
        <p className="coming-up-quiet">Saved queries need the desktop app.</p>
      ) : result.isLoading ? (
        <p className="coming-up-quiet" role="status">
          Loading…
        </p>
      ) : result.isError || !result.data ? (
        <p className="coming-up-quiet" role="alert">
          {result.error instanceof Error ? result.error.message : 'This query is unavailable.'}
        </p>
      ) : (
        <LedgerResult call={{ name: pin.name, arguments: pin.arguments, result: result.data }} />
      )}
    </Card>
  )
}

/** Queries pinned from an answer, kept current by native code. */
export function PinnedQueries() {
  const pins = usePins()
  if (!pins.length) return null
  return (
    <section className="pinned-grid" aria-label="Pinned">
      {pins.map((pin) => (
        <PinnedCard
          key={pin.id}
          pin={pin}
          onRemove={() => savePins(pins.filter(({ id }) => id !== pin.id))}
        />
      ))}
    </section>
  )
}
