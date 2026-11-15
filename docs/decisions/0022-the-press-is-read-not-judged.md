# 0022: The press is read, not judged

2026-11-14 · Accepted

## Context

The summary told a holding's move and quoted why it moved, but not how its coverage read, which a long-term holder following the news wants. Research on media tone finds it predicts little for large stocks and that pessimistic coverage pushes prices briefly before they recover, so tone is context, not a signal. Tested on saved stories, Apple's on-device model read tone plausibly for stories about the company, but couldn't tell which company a story was about and declined about one story in eight.

## Decision

- Rust decides which stories are about a holding (`news::relevance`, now also discounting a held firm's research or trading notices about other companies). Only those are read.
- Apple's model reads each such story once for its stance toward the company (positive, negative, mixed, or neutral) and its main sentence (`reading::read_stories`). A story it declines is saved as declined and counted unread.
- Rust counts the stances since the period began against the weeks before (`summary::press`) and writes the line: "Since Nov 6, stories about NBIS: 7 of 9 read as negative and 2 as positive; in the weeks before, about 3 in 10 read as negative." The portfolio names only the holding whose press turned most, by at least a quarter of its stories.
- Analyst actions stay out.

## Consequences

- The model labels; Rust counts and writes. The line says "read as", so the stance is a reading of the press, never Brief's view or a forecast.
- Tone accuracy hasn't been measured against hand labels; until it is, the line needs at least three read stories and compares only against at least five before.
- About six relevant stories a day across a typical portfolio, each read once in one to three seconds on this Mac.
