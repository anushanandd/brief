import { createContext, useContext } from 'react'

/**
 * Where floating popups portal to. A modal <dialog> renders in the browser's top layer,
 * so popups opened inside it must portal into the dialog or they appear behind it.
 */
export const PopupContainerContext = createContext<HTMLElement | null>(null)

/**
 * A popup in a container positions against the viewport. Absolute positioning would measure
 * against the dialog, which misplaces the first frame outside it and lets focus scroll it.
 */
export function usePopupPortal() {
  const container = useContext(PopupContainerContext) ?? undefined
  return { container, positionMethod: container ? ('fixed' as const) : ('absolute' as const) }
}
