import type { AsyncRouteComponent } from '@tanstack/react-router'
import { createElement, useSyncExternalStore } from 'react'

import { amountsHidden, subscribeAmountsHidden } from '../lib/format'

/**
 * Whether screen privacy masks amounts. Reading it re-renders the caller when the mask changes,
 * even under `memo`, so a view that caches formatted amounts can name it as a dependency.
 */
export const useAmountsHidden = () =>
  useSyncExternalStore(subscribeAmountsHidden, amountsHidden, amountsHidden)

/**
 * A route's page, rendered again whenever the mask changes. The router memoizes its matches,
 * so a page wouldn't otherwise format its amounts again; rendering in place keeps its drafts,
 * paging, and scroll, which remounting it would drop.
 */
export function maskedPage<TProps extends object>(
  Page: AsyncRouteComponent<TProps>,
): AsyncRouteComponent<TProps> {
  function MaskedPage(props: TProps) {
    useAmountsHidden()
    return createElement(Page, props)
  }
  MaskedPage.preload = Page.preload
  return MaskedPage
}
