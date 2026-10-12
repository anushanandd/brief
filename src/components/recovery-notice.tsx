import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'

import { financeQueryKey } from '../hooks/use-finance'
import { recoverFinanceState } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'
import { FilePlus, History } from './icons'
import { Button } from './ui'

type Recovery = NonNullable<FinanceSnapshot['recovery']>

/**
 * The saved database could not be read: restore the retained snapshot, or start a new one
 * after an in-app confirmation. Neither touches original files or Keychain credentials.
 */
export function RecoveryNotice({ recovery }: { recovery: Recovery }) {
  const client = useQueryClient()
  const [confirming, setConfirming] = useState(false)
  const trigger = useRef<HTMLButtonElement>(null)
  const cancelButton = useRef<HTMLButtonElement>(null)
  const recover = useMutation({
    mutationFn: recoverFinanceState,
    onSuccess: (snapshot) => {
      setConfirming(false)
      client.setQueryData(financeQueryKey, snapshot)
      void client.invalidateQueries({ queryKey: ['transaction-annotations'] })
      void client.invalidateQueries({ queryKey: ['market-snapshots'] })
    },
  })
  useEffect(() => {
    if (confirming) cancelButton.current?.focus()
  }, [confirming])
  const cancel = () => {
    setConfirming(false)
    recover.reset()
    trigger.current?.focus()
  }
  return (
    <section className="sync-notice" aria-label="Local data recovery" aria-busy={recover.isPending}>
      <h2>Local data needs recovery</h2>
      <p>{recovery.message}</p>
      <div className="recovery-actions">
        {recovery.canRestore ? (
          <Button
            icon={History}
            variant="primary"
            disabled={recover.isPending || confirming}
            onClick={() => recover.mutate(true)}
          >
            {recover.isPending && recover.variables ? 'Restoring…' : 'Restore retained snapshot'}
          </Button>
        ) : null}
        <Button
          ref={trigger}
          icon={FilePlus}
          variant="destructive"
          disabled={recover.isPending}
          aria-expanded={confirming}
          aria-controls={confirming ? 'new-snapshot-confirmation' : undefined}
          onClick={() => {
            recover.reset()
            setConfirming(true)
          }}
        >
          Start new snapshot
        </Button>
      </div>
      {confirming ? (
        <div
          id="new-snapshot-confirmation"
          className="connection-confirmation recovery-confirmation"
          role="group"
          aria-labelledby="new-snapshot-confirmation-title"
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              event.stopPropagation()
              if (!recover.isPending) cancel()
            }
          }}
        >
          <strong id="new-snapshot-confirmation-title">Start a new empty local snapshot?</strong>
          <p>Original files and Keychain credentials are kept.</p>
          <div>
            <Button ref={cancelButton} disabled={recover.isPending} onClick={cancel}>
              Cancel
            </Button>
            <Button
              icon={FilePlus}
              variant="destructive"
              disabled={recover.isPending}
              onClick={() => recover.mutate(false)}
            >
              {recover.isPending ? 'Starting…' : 'Confirm start'}
            </Button>
          </div>
        </div>
      ) : null}
      {recover.isError ? <p role="alert">{String(recover.error)}</p> : null}
    </section>
  )
}
