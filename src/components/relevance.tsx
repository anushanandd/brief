import { RELEVANT } from '../lib/holding-news'

/**
 * How much a story is about its company, from 0 to 100, as Rust scored it. Below `RELEVANT` it
 * only mentions the company, so the badge goes quiet.
 */
export function Relevance({ value }: { value: number }) {
  return (
    <span
      className={`news-relevance${value < RELEVANT ? ' news-relevance-low' : ''}`}
      title={`Relevance ${value} of 100`}
      aria-label={`Relevance ${value} of 100`}
    >
      {value}
    </span>
  )
}
