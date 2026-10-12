import { Slider } from '@base-ui/react/slider'
import { useEffect, useState } from 'react'

import { dayKey, dayMs, dayStart } from '../lib/dates'
import { formatDay } from '../lib/format'

export function ActivityDateSlider({
  minDate,
  maxDate,
  from,
  to,
  onChange,
}: {
  minDate: string
  maxDate: string
  from?: string
  to?: string
  onChange: (from: string, to: string) => void
}) {
  const min = dayStart(minDate) / dayMs
  const max = dayStart(maxDate) / dayMs
  const requestedStart = from ? dayStart(from) / dayMs : min
  const requestedEnd = to ? dayStart(to) / dayMs : max
  const start = Math.max(min, Math.min(max, requestedStart))
  const end = Math.max(start, Math.min(max, requestedEnd))
  const [values, setValues] = useState<number[]>([start, end])

  useEffect(() => setValues([start, end]), [start, end])

  if (min === max)
    return <span className="ledger-date-single">Only {formatDay(minDate)} available</span>

  return (
    <Slider.Root
      className="ledger-date-slider"
      value={values}
      onValueChange={setValues}
      min={min}
      max={max}
      step={1}
      thumbAlignment="edge"
      thumbCollisionBehavior="none"
      onValueCommitted={(next) => onChange(dayKey(next[0] * dayMs), dayKey(next[1] * dayMs))}
    >
      <Slider.Value className="ledger-date-slider-values">
        {(_, current) => (
          <>
            <span>{formatDay(dayKey(current[0] * dayMs))}</span>
            <span>{formatDay(dayKey(current[1] * dayMs))}</span>
          </>
        )}
      </Slider.Value>
      <Slider.Control className="ledger-date-slider-control">
        <Slider.Track className="ledger-date-slider-track">
          <Slider.Indicator className="ledger-date-slider-indicator" />
          <Slider.Thumb
            className="ledger-date-slider-thumb"
            index={0}
            getAriaLabel={() => 'Start date'}
            getAriaValueText={(_, value) => formatDay(dayKey(value * dayMs))}
          />
          <Slider.Thumb
            className="ledger-date-slider-thumb"
            index={1}
            getAriaLabel={() => 'End date'}
            getAriaValueText={(_, value) => formatDay(dayKey(value * dayMs))}
          />
        </Slider.Track>
      </Slider.Control>
    </Slider.Root>
  )
}
