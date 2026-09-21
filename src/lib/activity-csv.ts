import { invoke } from '@tauri-apps/api/core'

import type { ActivityItem } from './activity'
import { isTauri } from './api'

export function activityCsv(activities: ActivityItem[]) {
  return (
    [
      [
        'ID',
        'Account ID',
        'Date',
        'Type',
        'Description',
        'Category',
        'Detail',
        'Amount',
        'Pending',
      ],
      ...activities.map((item) => [
        item.id,
        item.accountId,
        item.date,
        item.kind,
        item.title,
        item.category,
        item.description ? `${item.detail} · ${item.description}` : item.detail,
        item.amount,
        item.pending,
      ]),
    ]
      .map((row) =>
        row
          .map((value) => {
            let text = value == null ? '' : String(value)
            // Quote fields and prevent spreadsheet applications from executing provider text.
            if (typeof value === 'string' && /^[\s]*[=+\-@\t\r]/.test(text)) text = `'${text}`
            return `"${text.replaceAll('"', '""')}"`
          })
          .join(','),
      )
      .join('\r\n') + '\r\n'
  )
}

export function activityCsvFilename(accountName: string) {
  const name = accountName
    .replace(/[\p{Cc}/\\:*?"<>|]/gu, '-')
    .replace(/^\.+/, '')
    .trim()
    .slice(0, 60)
  return `${name || 'Account'}-activity.csv`
}

export async function downloadActivityCsv(
  activities: ActivityItem[],
  accountId: string,
  accountName: string,
) {
  const contents = activityCsv(
    accountId === 'all' ? activities : activities.filter((item) => item.accountId === accountId),
  )
  const filename = activityCsvFilename(accountName)
  if (isTauri()) return invoke<boolean>('export_finance_csv', { contents, filename })
  const url = URL.createObjectURL(new Blob([contents], { type: 'text/csv;charset=utf-8' }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
  return true
}
