import { useQuery } from '@tanstack/react-query'
import { useCallback, useEffect, useRef, useState } from 'react'

import {
  generateFoundationExplanation,
  getFoundationModelStatus,
  isTauri,
  type FoundationAnswer,
} from '../lib/api'
import { errorMessage } from '../lib/errors'

/**
 * The palette's Ask mode: one question at a time to Apple Intelligence on this Mac, answered
 * from read-only ledger tools over the saved snapshot. Changing the question abandons the
 * request in flight.
 */
export function useAskBrief(active: boolean, hasData: boolean) {
  const [answer, setAnswer] = useState<FoundationAnswer & { question: string }>()
  const [error, setError] = useState<string>()
  const [asking, setAsking] = useState(false)
  const request = useRef<AbortController>(undefined)
  const model = useQuery({
    queryKey: ['foundation-model-status'],
    queryFn: getFoundationModelStatus,
    enabled: active && isTauri() && hasData,
    staleTime: 60_000,
  })
  const canAsk = hasData && model.data?.state === 'available'

  const reset = useCallback(() => {
    request.current?.abort()
    request.current = undefined
    setAsking(false)
    setAnswer(undefined)
    setError(undefined)
  }, [])
  useEffect(() => () => request.current?.abort(), [])

  const ready = (question: string) => canAsk && question.length >= 4 && !asking
  const ask = async (question: string) => {
    if (!ready(question)) return
    request.current?.abort()
    const controller = new AbortController()
    request.current = controller
    setAsking(true)
    setAnswer(undefined)
    setError(undefined)
    try {
      const response = await generateFoundationExplanation(question, controller.signal)
      setAnswer({ ...response, question })
    } catch (failure) {
      if (!controller.signal.aborted) setError(errorMessage(failure))
    } finally {
      if (!controller.signal.aborted) setAsking(false)
    }
  }

  /** Why a question can't be asked yet, or what is happening to it; `undefined` when ready. */
  const statusFor = (question: string) =>
    !isTauri()
      ? 'Apple Intelligence requires the desktop app'
      : !hasData
        ? 'Load a snapshot to ask Brief.'
        : model.isPending
          ? 'Checking Apple Intelligence…'
          : !canAsk
            ? (model.data?.message ?? 'Apple Intelligence is unavailable.')
            : asking
              ? 'Thinking on this Mac…'
              : question.length < 4
                ? 'Ask about your saved finances. Answers are generated on this Mac.'
                : undefined

  return { answer, error, asking, ready, ask, reset, statusFor }
}
