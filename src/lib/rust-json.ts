/**
 * JSON Rust wrote (a fixture its tests generate, or a message it assembles by hand), read as the
 * Rust type it serializes. It checks nothing, so it is only for data Rust itself produced; this
 * is the one place the renderer asserts a type rather than reading it.
 */
// oxlint-disable-next-line typescript/no-unnecessary-type-parameters, typescript/no-unsafe-type-assertion
export const fromRust = <T>(value: unknown): T => value as T
