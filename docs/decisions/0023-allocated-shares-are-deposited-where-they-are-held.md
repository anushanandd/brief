# 0023: Allocated shares are deposited where they are held

2026-11-14 · Accepted · Refines [0008](0008-rebuilt-brokerage-history-wins.md)

## Context

A stock plan's custodian can record its share allocations (an ESPP purchase, an RSU vest) in a linked brokerage's activity while the shares sit in the plan account. Brief counted each allocation as a deposit into the account that recorded it, and the Plaid plan, which has no tax lots, also borrowed those records as its own acquisitions. So every allocation was deposited twice in the combined brokerages. The recording brokerage's rebuild also failed, because undoing the allocations left it with fewer than no shares, and Brief fell back to the provider's history, which showed negative values. The plan's deposits were also priced at that day's close, so an ESPP discount read as money put in rather than gain.

## Decision

- A brokerage counts an allocation only when its own records hold the shares: undoing its later trades and allocations from today's shares must never leave fewer than none of that ticker. Otherwise the allocation is left out of its flows and its rebuild.
- A stock plan still takes its acquisitions from its tax lots, or else from the allocations any brokerage recorded.
- A stock plan's deposits are what its shares cost: the holding's cost basis, spread over its acquisitions in proportion to each one's value at that day's close. Without a cost basis, or with an acquisition Alpaca can't price, they are the closes.
- Allocations that no account holds, and no connected plan holds either, are no one's deposit, and the refresh warns.

## Consequences

- The rule depends only on each account's own records. It holds for plans with or without tax lots, for plans that aren't connected, and for brokerages that really hold the vested shares.
- A ticker's allocations are all counted or none. One moved out of the account in part by an unrecorded transfer would be dropped entirely.
- Spreading the cost basis by closing value is exact when every purchase has the same discount. A lookback ESPP that bought at different discounts gets their average.
