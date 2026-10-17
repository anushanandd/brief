# Principles

Brief turns the records of your life into understanding you can trust. It runs on your Mac, reads only what you allow, and shows its work.

These principles decide what to build and how. When two of them pull apart, the earlier one wins.

1. **Your data stays yours.** Nothing leaves the Mac without your say, and providers are read-only. Trust is the product; a finance app that leaks once is finished.
2. **True before useful.** Every figure is observed, derived from observed values, explained by a cited rule, or marked as a suggestion. A missing value stays missing and an incomplete total says so. One invented number costs the trust a hundred correct ones earned.
3. **You have the last word.** Your edits outrank every source and survive every refresh. Brief advises; you decide.
4. **Clear before complete.** Everything Brief stores is reachable, but only what matters is prominent. Detail sits one click away, and identifiers one deliberate click beyond that.
5. **Fast is a feature.** Reads are local and nothing waits on the network. A page that takes a second feels broken.
6. **Calm and beautiful.** Brief stays quiet until something needs you. Motion explains a change or it doesn't happen.

## Hard constraints

These hold without exception. Each is enforced by a test, or should be.

- Credentials and provider tokens live only in macOS Keychain.
- Rust computes every financial result. The interface formats values; it never sums, rounds, or classifies money. Drawing Rust's figures (a chart's scale, positions, and stacking) is not a result, an edit form may check the amounts typed into it before Rust validates them, and a security's public price is not your money.
- No billed provider call runs unless you turned it on.
- A refresh commits completely or not at all, and a failure leaves the last good state in place.
- Recorded history is appended, never rewritten.
- Real financial data never appears in fixtures, logs, screenshots, or tests.

## When a case isn't covered

Find the earliest principle the choice touches, pick what serves it best, and say why in the commit message. If the reason is worth keeping, it belongs here.
