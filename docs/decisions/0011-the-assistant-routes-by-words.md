# 0011: The assistant routes questions by words

Recorded 2026-11-11 from the earlier architecture docs; decided before then, date not recorded · Accepted

## Context

Apple's on-device model reads about 4,096 tokens per question, tool definitions included, so it can be offered only a few tools. Choosing them with the question's embedding would mean a native call per question that tests can't run.

## Decision

`insights::assistant_planner` scores tools by the question's words and the ledger's names, resolves its dates, and offers only the highest. A question with one clear query is answered by Rust running it, without the model choosing.

## Consequences

Routing is deterministic and tested over the eval set (`insights/assistant-evals.jsonl`) without the model. It doesn't match by meaning, so a new tool needs hint words, and an eval checks them.
