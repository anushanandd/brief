import { ledgerView, type LedgerCall } from '../lib/ledger-view'

/** The figures of one native ledger query: single values, then a short table. */
export function LedgerResult({ call }: { call: LedgerCall }) {
  const view = ledgerView(call)
  return (
    <div className="ledger-result">
      {view.facts.length ? (
        <dl className="ledger-result-facts">
          {view.facts.map((fact) => (
            <div key={fact.label}>
              <dt>{fact.label}</dt>
              <dd className={fact.sensitive ? 'sensitive-value' : undefined}>{fact.text}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {view.table ? (
        <>
          <table className="ledger-result-table">
            <thead>
              <tr>
                {view.table.columns.map((column) => (
                  <th scope="col" key={column}>
                    {column}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {view.table.rows.map((row, index) => (
                <tr key={index}>
                  {row.map((cell, column) => (
                    <td key={column} className={cell.sensitive ? 'sensitive-value' : undefined}>
                      {cell.text}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
          {view.table.more ? (
            <p className="ledger-result-more">{view.table.more} more not shown</p>
          ) : null}
        </>
      ) : null}
    </div>
  )
}
