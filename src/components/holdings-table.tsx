import { createColumnHelper, tableFeatures, useTable } from '@tanstack/react-table'

import { formatCurrency, formatPercent } from '../lib/format'
import { stockLogoUrl } from '../lib/logos'
import type { Holding } from '../lib/schema'
import { BrandMark } from './brand-mark'

const features = tableFeatures({})
const helper = createColumnHelper<typeof features, Holding>()
const columns = helper.columns([
  helper.accessor('ticker', {
    header: 'Asset',
    cell: ({ row }) => (
      <div className="table-asset">
        <BrandMark
          className="asset-mark"
          fallback={row.original.ticker.slice(0, 1)}
          label={`${row.original.name} logo`}
          src={stockLogoUrl(row.original.ticker)}
          style={{ backgroundColor: row.original.color }}
        />
        <span>
          <strong>{row.original.ticker}</strong>
          <small>{row.original.name}</small>
        </span>
      </div>
    ),
  }),
  helper.accessor('shares', {
    header: 'Shares',
    cell: ({ getValue }) => getValue().toLocaleString('en-US'),
  }),
  helper.accessor('price', {
    header: 'Price',
    cell: ({ getValue }) => formatCurrency(getValue()),
  }),
  helper.accessor('value', {
    header: 'Market value',
    cell: ({ getValue }) => <strong>{formatCurrency(getValue())}</strong>,
  }),
  helper.accessor('dailyChangePct', {
    header: 'Today',
    cell: ({ getValue }) => (
      <span className={getValue() >= 0 ? 'positive' : 'negative'}>{formatPercent(getValue())}</span>
    ),
  }),
  helper.accessor('totalChangePct', {
    header: 'Total return',
    cell: ({ getValue }) => (
      <span className={getValue() >= 0 ? 'positive' : 'negative'}>{formatPercent(getValue())}</span>
    ),
  }),
])

export function HoldingsTable({ data }: { data: Holding[] }) {
  const table = useTable({ features, columns, data })

  return (
    <div className="table-scroll">
      <table className="holdings-table">
        <thead>
          {table.getHeaderGroups().map((group) => (
            <tr key={group.id}>
              {group.headers.map((header) => (
                <th key={header.id}>
                  {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr key={row.id}>
              {row.getAllCells().map((cell) => (
                <td key={cell.id}>
                  <table.FlexRender cell={cell} />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
