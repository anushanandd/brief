import { Slider } from '@base-ui/react/slider'
import { useEffect, useState } from 'react'

import { formatActivityDate } from '../lib/activity-filters'

const dayMs = 86_400_000

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
  const min = Date.parse(`${minDate}T00:00:00Z`) / dayMs
  const max = Date.parse(`${maxDate}T00:00:00Z`) / dayMs
  const requestedStart = from ? Date.parse(`${from}T00:00:00Z`) / dayMs : min
  const requestedEnd = to ? Date.parse(`${to}T00:00:00Z`) / dayMs : max
  const start = Math.max(min, Math.min(max, requestedStart))
  const end = Math.max(start, Math.min(max, requestedEnd))
  const [values, setValues] = useState<number[]>([start, end])

  useEffect(() => setValues([start, end]), [start, end])

  if (min === max)
    return <span className="ledger-date-single">Only {formatActivityDate(minDate)} available</span>

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
      onValueCommitted={(next) =>
        onChange(
          new Date(next[0] * dayMs).toISOString().slice(0, 10),
          new Date(next[1] * dayMs).toISOString().slice(0, 10),
        )
      }
    >
      <Slider.Value className="ledger-date-slider-values">
        {(_, current) => (
          <>
            <span>
              {formatActivityDate(new Date(current[0] * dayMs).toISOString().slice(0, 10))}
            </span>
            <span>
              {formatActivityDate(new Date(current[1] * dayMs).toISOString().slice(0, 10))}
            </span>
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
            getAriaValueText={(_, value) =>
              formatActivityDate(new Date(value * dayMs).toISOString().slice(0, 10))
            }
          />
          <Slider.Thumb
            className="ledger-date-slider-thumb"
            index={1}
            getAriaLabel={() => 'End date'}
            getAriaValueText={(_, value) =>
              formatActivityDate(new Date(value * dayMs).toISOString().slice(0, 10))
            }
          />
        </Slider.Track>
      </Slider.Control>
    </Slider.Root>
  )
}
