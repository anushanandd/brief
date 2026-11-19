import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useRef, useState } from 'react'

import { setFinanceSnapshot } from '../hooks/use-finance'
import { recoverFinanceState } from '../lib/api'
import { errorMessage } from '../lib/errors'
import type { FinanceSnapshot } from '../lib/schema'
import { FilePlus, History } from './icons'
import { InlineConfirm } from './settings-ui'
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
  const recover = useMutation({
    mutationFn: recoverFinanceState,
    onSuccess: (snapshot) => {
      setConfirming(false)
      setFinanceSnapshot(client, snapshot)
      void client.invalidateQueries({ queryKey: ['transaction-annotations'] })
    },
  })
  const cancel = () => {
    setConfirming(false)
    recover.reset()
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
        <InlineConfirm
          id="new-snapshot-confirmation"
          className="recovery-confirmation"
          labelledBy="new-snapshot-confirmation-title"
          pending={recover.isPending}
          onCancel={cancel}
          returnFocus={() => trigger.current}
          actions={
            <Button
              icon={FilePlus}
              size="compact"
              variant="destructive"
              disabled={recover.isPending}
              onClick={() => recover.mutate(false)}
            >
              {recover.isPending ? 'Starting…' : 'Confirm start'}
            </Button>
          }
        >
          <strong id="new-snapshot-confirmation-title">Start a new empty local snapshot?</strong>
          <p>Original files and Keychain credentials are kept.</p>
        </InlineConfirm>
      ) : null}
      {recover.isError ? <p role="alert">{errorMessage(recover.error)}</p> : null}
    </section>
  )
}
