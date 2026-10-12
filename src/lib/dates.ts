/**
 * Calendar days as `YYYY-MM-DD` keys. Arithmetic runs on UTC midnights, so a day never shifts
 * with the clock or a daylight-saving change.
 */

export const dayMs = 86_400_000

/** The UTC day of an instant, given in milliseconds or as a date. */
export const dayKey = (time: number | Date) => new Date(time).toISOString().slice(0, 10)

/** Midnight UTC of a day, in milliseconds. */
export const dayStart = (date: string) => Date.parse(`${date}T00:00:00Z`)

/** The day `days` after `date`, or before it when negative. */
export const addDays = (date: string, days: number) => dayKey(dayStart(date) + days * dayMs)

/** Whole days from `from` to `to`; negative when `to` is earlier. */
export const daysBetween = (from: string, to: string) =>
  Math.round((dayStart(to) - dayStart(from)) / dayMs)

/** The local calendar day of a time given in seconds. */
export const localDateKey = (seconds: number) => {
  const date = new Date(seconds * 1000)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

/** Whether a year, month (1 to 12), and day name a real calendar day. */
export function isCalendarDay(year: number, month: number, day: number) {
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0)
  const days = month === 2 ? (leap ? 29 : 28) : [4, 6, 9, 11].includes(month) ? 30 : 31
  return (
    Number.isInteger(year) &&
    Number.isInteger(day) &&
    month >= 1 &&
    month <= 12 &&
    day >= 1 &&
    day <= days
  )
}

/** `value` when it is a real calendar day written `YYYY-MM-DD`. */
export function validDate(value: unknown) {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return undefined
  const [year, month, day] = value.split('-').map(Number)
  return isCalendarDay(year, month, day) ? value : undefined
}
