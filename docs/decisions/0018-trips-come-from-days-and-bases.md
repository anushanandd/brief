# 0018: Trips come from days and bases

2026-11-13 · Accepted

## Context

Trips used to compare each in-store purchase with one fixed home: the states that held a share of the last year's purchase days, applied to two years of history. That went wrong in three ways:

- **Where a merchant bills is not where you were.** A laundry machine operator or a vending company prints its head office's city. One or two such charges a day apart made a trip to a city the user never visited, and trips looked the place up again after the transaction policy had deliberately left it out.
- **Home moves.** A semester abroad became one long trip with its weekend trips folded in. College was home only while it was in the last year; once it left that window, its months would have become trips.
- **Silence ended trips.** A week on an island with two purchases split in two, and a one-purchase day trip didn't count, though the phone's location placed both.

## Decision

Trips are worked out in steps, each of which can be tested on its own (`insights/trips`):

1. **Evidence of where the user was.** Each in-person purchase with coordinates is a piece of evidence. One that Wallet located by the phone weighs more than one located by a merchant's printed city. Flights and stays in the user's own calendar count too ([0019](0019-travel-reservations-reach-trips.md)). Trips use the transaction's place as the policy set it and never look one up again.
2. **Days.** Evidence within 100 km is one area. Each day goes to the area its evidence weighs most.
3. **Bases.** An area is a base for any stretch with evidence on at least 8 days across at least 28, with no gap longer than 150 days. Bases come from all of the history, so a former home stays home for its own time.
4. **Trips.** A trip is a run of days away from every base active then. A day at a base ends a trip; silence ends one only after 7 days. A trip needs strong evidence (the phone or a reservation), or purchases from two different merchants, so one merchant's billing city can't make one.

In Wallet, the phone outranks the merchant's records: a Maps match more than 1 km from the phone is another business, and the city Wallet worked out from the phone beats the city the card network printed.

A trip also claims flights and lodging charged up to 60 days before it, with a confidence that rises the closer the booking was and when the calendar holds the trip's travel. Its total splits into getting there, staying, and on the ground.

## Consequences

- The thresholds were checked against one owner's real history; every value nearby gave the same trips.
- A move takes about four weeks to become a base. Until then the new home looks like an ongoing trip.
- Purchases on cards without Wallet's phone location need a second merchant, so a one-purchase day trip on such a card is missed unless the calendar has it.
- A booking is linked by time alone, so a flight bought for a later trip can join an earlier one if it falls within the window.
