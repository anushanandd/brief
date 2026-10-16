import type { BriefSpan } from '../lib/schema'

const toneClass = { up: 'positive', down: 'negative', flat: '' } as const

/** One span as Rust wrote it: React styles it and adds no words of its own. */
function SpanText({ span }: { span: BriefSpan }) {
  if (span.kind === 'text') return span.text
  if (span.kind === 'subject') return <strong>{span.text}</strong>
  return (
    <strong
      className={[toneClass[span.tone], span.sensitive ? 'sensitive-value' : '']
        .filter(Boolean)
        .join(' ')}
    >
      {span.text}
    </strong>
  )
}

/** A line of the day's brief, its spans in order. */
export function BriefLine({ spans }: { spans: BriefSpan[] }) {
  return (
    <>
      {spans.map((span, index) => (
        <SpanText key={index} span={span} />
      ))}
    </>
  )
}
