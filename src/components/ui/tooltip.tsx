import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip'
import { createContext, useContext, useState } from 'react'

const TooltipPointerContext = createContext<{
  pointer: { x: number; y: number } | null
  setPointer: (pointer: { x: number; y: number } | null) => void
} | null>(null)

function TooltipProvider({ delay = 0, ...props }: TooltipPrimitive.Provider.Props) {
  return <TooltipPrimitive.Provider data-slot="tooltip-provider" delay={delay} {...props} />
}

function Tooltip(props: TooltipPrimitive.Root.Props) {
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null)
  return (
    <TooltipPointerContext value={{ pointer, setPointer }}>
      <TooltipPrimitive.Root data-slot="tooltip" {...props} />
    </TooltipPointerContext>
  )
}

function TooltipTrigger({ onFocus, onPointerMove, ...props }: TooltipPrimitive.Trigger.Props) {
  const context = useContext(TooltipPointerContext)
  return (
    <TooltipPrimitive.Trigger
      data-slot="tooltip-trigger"
      onFocus={(event) => {
        context?.setPointer(null)
        onFocus?.(event)
      }}
      onPointerMove={(event) => {
        context?.setPointer({ x: event.clientX, y: event.clientY })
        onPointerMove?.(event)
      }}
      {...props}
    />
  )
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
  const pointer = useContext(TooltipPointerContext)?.pointer
  const pointerAnchor = pointer
    ? { getBoundingClientRect: () => DOMRect.fromRect({ x: pointer.x, y: pointer.y }) }
    : undefined
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Positioner
        className="tooltip-positioner"
        align={align}
        alignOffset={alignOffset}
        anchor={pointerAnchor ?? anchor}
        side={side}
        sideOffset={sideOffset}
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
