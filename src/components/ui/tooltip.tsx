import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip'

function TooltipProvider({ delay = 0, ...props }: TooltipPrimitive.Provider.Props) {
  return <TooltipPrimitive.Provider data-slot="tooltip-provider" delay={delay} {...props} />
}

function Tooltip(props: TooltipPrimitive.Root.Props) {
  return <TooltipPrimitive.Root data-slot="tooltip" {...props} />
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
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Positioner
        className="tooltip-positioner"
        align={align}
        alignOffset={alignOffset}
        anchor={anchor}
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
