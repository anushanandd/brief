const modifiers: Record<string, string> = { '⌘': '⌘', '⌥': '⌥', '⇧': '⇧', '⌃': '⌃', '^': '⌃' }

/**
 * Splits a binding label into key caps: chords are separated by spaces ("g f"), and leading
 * modifier symbols each get their own cap ("⇧⌘O" → ⇧ ⌘ O).
 */
export function keyCaps(label: string): string[][] {
  return label
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((chord) => {
      const caps: string[] = []
      let rest = chord
      while (rest.length > 1 && modifiers[rest[0]]) {
        caps.push(modifiers[rest[0]])
        rest = rest.slice(1)
      }
      caps.push(rest.toLowerCase() === 'esc' ? 'Esc' : rest)
      return caps
    })
}

/** A shortcut drawn as one key cap per key, as in the Keyboard shortcuts dialog. */
export function Keys({ label, className }: { label: string; className?: string }) {
  return (
    <span className={className ? `keys ${className}` : 'keys'}>
      {keyCaps(label).map((chord, chordIndex) => (
        <span className="keys-chord" key={chordIndex}>
          {chord.map((cap, capIndex) => (
            <kbd className="keycap" key={capIndex}>
              {cap}
            </kbd>
          ))}
        </span>
      ))}
    </span>
  )
}
