export const selectedFilterValues = (query: string | undefined, options: string[]) =>
  (query ?? '').split(',').filter((value) => options.includes(value))
