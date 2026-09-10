const spendingAccountKey = 'brief:spending-account-id'
const hiddenPlatinumBenefitsKey = 'brief:hidden-platinum-benefits'

export const defaultHiddenPlatinumBenefitIds = [
  'walmart-plus',
  'clear',
  'equinox',
  'oura',
  'uber-one',
]

export const getSpendingAccountId = () => localStorage.getItem(spendingAccountKey) ?? ''

export const saveSpendingAccountId = (accountId: string) => {
  if (accountId) localStorage.setItem(spendingAccountKey, accountId)
  else localStorage.removeItem(spendingAccountKey)
}

export const parseHiddenPlatinumBenefitIds = (value: unknown) =>
  typeof value === 'string'
    ? value.split(',').filter(Boolean)
    : [...defaultHiddenPlatinumBenefitIds]

export const getHiddenPlatinumBenefitIds = () =>
  parseHiddenPlatinumBenefitIds(localStorage.getItem(hiddenPlatinumBenefitsKey))

export const saveHiddenPlatinumBenefitIds = (ids: string[]) =>
  localStorage.setItem(hiddenPlatinumBenefitsKey, ids.join(','))
