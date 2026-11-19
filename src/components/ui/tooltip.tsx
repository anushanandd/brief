import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip'
import { createContext, useContext, useState } from 'react'

import { usePopupPortal } from './popup-container'

const TooltipPointerContext = createContext<{ x: number; y: number } | null>(null)

function TooltipProvider({ delay = 0, ...props }: TooltipPrimitive.Provider.Props) {
  return <TooltipPrimitive.Provider data-slot="tooltip-provider" delay={delay} {...props} />
}

function Tooltip({ onOpenChange, onOpenChangeComplete, ...props }: TooltipPrimitive.Root.Props) {
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null)
  return (
    <TooltipPointerContext value={pointer}>
      <TooltipPrimitive.Root
        data-slot="tooltip"
        // A hover opens it where the pointer is as it opens; focus or a key opens it at the trigger.
        onOpenChange={(open, details) => {
          if (open)
            setPointer(
              details.reason === 'trigger-hover' && details.event instanceof MouseEvent
                ? { x: details.event.clientX, y: details.event.clientY }
                : null,
            )
          onOpenChange?.(open, details)
        }}
        // Cleared once it has closed, so a later open from a command starts at the trigger.
        onOpenChangeComplete={(open) => {
          if (!open) setPointer(null)
          onOpenChangeComplete?.(open)
        }}
        {...props}
      />
    </TooltipPointerContext>
  )
}

function TooltipTrigger(props: TooltipPrimitive.Trigger.Props) {
  return <TooltipPrimitive.Trigger data-slot="tooltip-trigger" {...props} />
}

function TooltipContent({
  className,
  side = 'top',
  sideOffset = 4,
  align = 'center',
  alignOffset = 0,
  anchor,
  children,
  ...props
}: Omit<TooltipPrimitive.Popup.Props, 'className'> & {
  className?: string
} & Pick<
    TooltipPrimitive.Positioner.Props,
    'align' | 'alignOffset' | 'anchor' | 'side' | 'sideOffset'
  >) {
  const pointer = useContext(TooltipPointerContext)
  const { container, positionMethod } = usePopupPortal()
  const pointerAnchor = pointer
    ? { getBoundingClientRect: () => DOMRect.fromRect({ x: pointer.x, y: pointer.y }) }
    : undefined
  return (
    <TooltipPrimitive.Portal container={container}>
      <TooltipPrimitive.Positioner
        className="tooltip-positioner"
        align={align}
        alignOffset={alignOffset}
        anchor={pointerAnchor ?? anchor}
        side={side}
        sideOffset={sideOffset}
        positionMethod={positionMethod}
      >
        <TooltipPrimitive.Popup
          data-slot="tooltip-content"
          className={`tooltip-content${className ? ` ${className}` : ''}`}
          {...props}
        >
          {children}
          <TooltipPrimitive.Arrow
            className="tooltip-arrow"
            render={
              <svg viewBox="0 0 10 5" preserveAspectRatio="none">
                <path d="M0 0h10L5 5Z" />
              </svg>
            }
          />
        </TooltipPrimitive.Popup>
      </TooltipPrimitive.Positioner>
    </TooltipPrimitive.Portal>
  )
}

export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger }
