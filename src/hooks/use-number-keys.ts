import { useMemo } from 'react'

import { accountMenuGroups } from '../components/all-accounts-overview'
import type { AppAction } from '../lib/action-system'
import { financeSettings, spendingAccountIds } from '../lib/finance-settings'
import { consolidatedPositions } from '../lib/holding-detail'
import { numberKeyDestinations } from '../lib/number-keys'
import { useFinance } from './use-finance'

/**
 * What a number key can open now: the pages and reports among `actions`, and the accounts,
 * types of account, and securities in the snapshot. It follows the snapshot, not live prices,
 * so it changes only when what exists does.
 */
export function useNumberKeyDestinations(actions: readonly AppAction[]) {
  const data = useFinance().data
  return useMemo(() => {
    const settings = financeSettings(data)
    return numberKeyDestinations({
      actions,
      groups: data ? accountMenuGroups(data, spendingAccountIds(settings, data.accounts), 0) : [],
      names: settings.accountNames,
      holdings: data ? consolidatedPositions(data) : [],
    })
  }, [actions, data])
}
