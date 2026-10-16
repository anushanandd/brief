/** What a failure says, whether it threw an `Error` or anything else. */
export const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error)
