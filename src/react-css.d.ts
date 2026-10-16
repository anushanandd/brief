import 'react'

declare module 'react' {
  interface CSSProperties {
    /** A custom property a stylesheet reads, such as `--pace-accent`. */
    [property: `--${string}`]: string | number | undefined
  }
}
