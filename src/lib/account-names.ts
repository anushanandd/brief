const maximumDisplayNameLength = 80

export type AccountDisplayNames = Record<string, string>

export function parseAccountDisplayNames(value: unknown): AccountDisplayNames {
  if (typeof value !== 'string') return {}
  try {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {}
    return Object.fromEntries(
      Object.entries(parsed).flatMap(([accountId, name]) => {
        if (!accountId || typeof name !== 'string') return []
        const normalized = name.trim().slice(0, maximumDisplayNameLength)
        return normalized ? [[accountId, normalized]] : []
      }),
    )
  } catch {
    return {}
  }
}

export function accountDisplayName(
  accountId: string | undefined,
  providerName: string,
  names: AccountDisplayNames,
) {
  return (accountId && names[accountId]?.trim()) || providerName
}
