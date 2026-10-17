import { useNavigate } from '@tanstack/react-router'
import { useCallback } from 'react'

import { useShowObject } from '../components/object-details'
import { useQueueAction } from '../lib/action-system'
import { getObject, openObject } from '../lib/objects'
import type { Verb } from '../lib/verbs'

/**
 * Runs a catalog verb from anywhere: opens its object's page (or its own page), then runs it
 * once that page registers it.
 */
export function useRunVerb() {
  const navigate = useNavigate()
  const queue = useQueueAction()
  const showObject = useShowObject()
  return useCallback(
    async (verb: Verb, objectId?: string) => {
      if (verb.object) {
        const detail = objectId ? await getObject(objectId) : null
        if (!detail) return
        await openObject(navigate, detail.object, showObject)
      } else if (verb.page) {
        await navigate({ href: verb.page })
      }
      queue(verb.id)
    },
    [navigate, queue, showObject],
  )
}
