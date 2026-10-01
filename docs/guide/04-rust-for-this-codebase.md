# 04 · Rust for this codebase

You don't need to "learn Rust" to read FinGuard; you need about twelve ideas. Each is shown with a real snippet from the repo and the bug class it eliminates. (If you want a full course afterwards: *The Rust Book*, then *Rust for Rustaceans*; for async, *Asynchronous Programming in Rust*; for sqlx, the docs.)

## 1. Static types that mean something (newtypes)

```rust
// crates/domain/src/ids.rs
pub struct AccountId(pub Uuid);
pub struct CardId(pub Uuid);
```
`AccountId` and `CardId` are both UUIDs inside, but the compiler treats them as unrelated types. A function `freeze(card: CardId)` can't be called with an account id. In JavaScript or Python both are "just strings" and mix-ups ship to production.

💥 Without it: "freeze the card" with an *account* id runs and silently does nothing (or the wrong thing).

## 2. `enum` — "one of these, and the compiler checks you handled each"

```rust
pub enum ProposedAction {
    ReadBalance { account_id: AccountId },
    FreezeCard  { card_id: CardId },
    TransferFunds { from_account: AccountId, recipient_id: RecipientId, amount: Money },
}
```
An enum value is exactly one variant, with that variant's data. `match` on it must cover every variant:

```rust
match action {
    ProposedAction::ReadBalance { .. } => RiskClass::ReadOnly,
    ProposedAction::FreezeCard  { .. } => RiskClass::Reversible,
    ProposedAction::TransferFunds { .. } => RiskClass::MoneyMovement,
}
```
Add a fourth variant and **every non-exhaustive `match` stops compiling**. That's how a new action can't ship without a risk class and a policy rule.

💥 In a string-typed design (`tool_name: "transfer"`) a new tool silently falls through an `if/else` and skips checks.

## 3. `Option<T>` instead of `null`

```rust
pub amount: Option<Money>,   // the draft may not have an amount yet
```
`Option` is `Some(value)` or `None`; you can't use the value without handling `None`. `ActionDraft` (optional amount) vs `ProposedAction` (required amount) is the type-level difference between "still being clarified" and "executable".

## 4. `Result<T, E>` and `?` — errors are values

```rust
let action = actions::get_for_update(&mut tx, p.action_id).await?;
```
Functions that can fail return `Result`. `?` means "if error, return it from this function now". No exceptions flying through the stack; the signature tells you what can fail. Our errors are *typed*: `DecisionProviderError::{Unauthorized, RateLimited, Timeout, ProviderUnavailable, InvalidResponse}` rather than "500 something went wrong".

`thiserror` derives the boilerplate (`#[error("…")]`).

## 5. Ownership and borrowing (just enough)

* A value has one owner. Passing it by value *moves* it.
* `&T` is a shared read-only borrow; `&mut T` is an exclusive writable borrow. You can have many `&T` **or** one `&mut T`.
* This is how Rust prevents data races at compile time.

You'll see `&ActorContext` (borrowed, read-only), `&mut PgConnection` (exclusive access to a DB connection), and `.clone()` where we need an owned copy. If the compiler complains, it's usually right — read the message, it's famously good.

## 6. Traits — interfaces

```rust
#[async_trait]
pub trait DecisionProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn assess(&self, ctx: &RiskContext) -> Result<RiskAssessment, DecisionProviderError>;
}
```
`JevDecisionProvider` and `MockDecisionProvider` both implement it. The agent holds an `Arc<dyn DecisionProvider>` ("a shared pointer to *something* that implements it"), so tests inject the mock and production injects the real one. `Send + Sync` means "safe to share across threads", required because our server is multi-threaded.

`#[async_trait]` is a macro that lets traits have `async fn` while still being usable as `dyn Trait`.

## 7. `async` / `await` and Tokio

`async fn` returns a future (a value representing work in progress). `.await` pauses *this* task until it's done while the thread serves other tasks. Tokio is the runtime that schedules them. Key rule: **don't hold scarce resources across an `.await` you don't need to** — see in `pipeline.rs` where we `drop(c)` the DB connection before calling Jev, so a slow HTTP call doesn't pin a pooled connection.

`tokio::spawn` starts a concurrent task (used heavily in the concurrency tests).

## 8. Serde — JSON ⇄ types

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw { intent_type: IntentKind, amount_minor: Option<i64>, ... }
```
`serde_json::from_str::<Raw>(text)` either gives you a fully valid typed value or an error. `deny_unknown_fields` rejects extra keys (so a model can't smuggle `"skip_confirmation": true`). Enum variants serialise by name (`"TRANSFER_FUNDS"`), and an unknown variant fails to deserialise — that's how `EXECUTE_TRANSFER` is impossible in `RecommendedHandling`.

## 9. SQLx — SQL with parameters

```rust
sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE").bind(id).fetch_one(&mut *tx).await?
```
Parameters are bound (never string-concatenated) → no SQL injection. We use the runtime `query`/`query_as` API (not the compile-time `query!` macro) so the project builds without a live database. `sqlx::Transaction` rolls back automatically if dropped without `commit()` — an early `return Err(..)` inside a transaction is a rollback.

## 10. `Arc` and shared state

`Arc<T>` is an atomically reference-counted pointer: cheap to clone, shared across tasks. The axum `AppState` holds `Arc<Agent>`. Mutable shared state needs a lock (`Mutex`) — used only in mocks.

## 11. Modules, crates, workspaces

A *crate* is a package; a *workspace* groups crates (`Cargo.toml` at the root). Dependencies are explicit per crate (`crates/policy/Cargo.toml` lists only `finguard-domain`) — **the compiler enforces the architecture diagram**: `policy` literally cannot call SQL because it doesn't depend on `sqlx`.

## 12. Testing is built in

`#[test]` for unit tests next to the code; `#[tokio::test]` for async; `#[sqlx::test]` creates a fresh database per test and runs migrations. `cargo test` runs everything. `cargo clippy -D warnings` and `cargo fmt` keep style honest.

---

## Reading exercise (15 minutes)

1. Open `crates/domain/src/state.rs`. Find `allowed_next`. Add a pretend transition `Executed → Executing` and run `cargo test -p finguard-domain`. Which test fails, and why is that good?
2. Open `crates/policy/src/decide.rs`. In `stage_one`, the five rules are an array of function pointers: `[rule_hard, rule_provider_failure, rule_incomplete, rule_severe, rule_confidence]`. Reorder two of them. Which test breaks?
3. In `crates/domain/src/money.rs`, change `parse_major` to allow 3 decimals. What fails?

## Common compiler messages and what they mean

| Message | Meaning | Typical fix |
|---|---|---|
| `borrow of moved value` | you used a value after giving it away | borrow (`&x`) or `.clone()` |
| `cannot borrow as mutable` | you need `&mut` / `mut` | add `mut` or restructure |
| `the trait bound … is not satisfied` | a type lacks an impl (e.g. `Send`) | implement/derive it, or change the type |
| `non-exhaustive patterns` | a `match` misses a variant | handle it (this is the feature!) |
| `future cannot be sent between threads safely` | holding a non-`Send` value across `.await` | drop it before awaiting |

Next: [05 · Run it and poke it](05-run-it-and-poke-it.md)
