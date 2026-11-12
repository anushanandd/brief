# 0006: Saved summaries refresh in place

Recorded 2026-11-11 from the earlier architecture docs; decided before then, date not recorded · Accepted

## Context

The investment summary reads market data, news, and model readings that take time to gather. A page shouldn't wait on data it has already summarized, and keying the summary's query on live prices had made the page reload on every tick.

## Decision

Each summary is saved with a hash of its inputs (`summary::inputs_key`), which leaves out live prices. When an input changes, the older summary is returned marked `refreshing` while a new one is made in the background, and the two are never mixed. Rust writes every sentence; a model only picks which numbered sentence of a saved text to quote, read in the background before use.

## Consequences

The page opens on a whole summary at once and never waits on a model or the network. A summary may be briefly out of date, and says so.
