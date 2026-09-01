# Brief

Brief is a private, local-first personal finance desktop app. The MVP combines a fast React interface with a small Rust core and on-device SQLite storage.

## MVP

- Dashboard with net worth, account switching, history, and top holdings
- Analytics for performance, allocation, and dividends
- Holdings with prices, after-hours values, search, and market-note placeholders
- Spending with statement balance, categories, transactions, and card credits
- Settings for appearance, provider health, credentials, and privacy preferences
- Keyboard-first navigation and a `Command-K` command palette
- Cached local data that renders before network refreshes

Plaid and SnapTrade use bring-your-own-key integrations in the native desktop process. Provider credentials and Plaid access tokens are stored in the macOS Keychain; no hosted bridge is required.

The browser development build uses a deterministic sample snapshot. The native desktop build can connect read-only provider accounts and keeps its financial snapshot on device. It is not suitable for making financial decisions.

## Run

```sh
pnpm install
pnpm dev
```

Run the native desktop shell:

```sh
pnpm tauri dev
```

In Brief → Settings, enter a Plaid client ID and Production secret and/or a SnapTrade Personal client ID and consumer key. Save and test the credentials, then choose the provider under Data sources to connect accounts. Secrets are never written to the frontend cache or SQLite.

Verify the project:

```sh
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml
```

## Documentation

- [Product specification](docs/general.md)
- [Architecture and stack](docs/stack.md)
- [MVP implementation](docs/mvp.md)
- [Research archive](docs/gpt-discussion.md)
