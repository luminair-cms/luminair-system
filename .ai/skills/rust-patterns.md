# Skill: Rust Design Patterns, Conventions & Idioms for Luminair

Load this skill before **designing, writing, modifying, or reviewing** any Rust code in this repository.
It unifies architectural patterns, Domain-Driven Design (DDD) idioms, classic GoF patterns in idiomatic Rust form, and Rust-specific language idioms.

---

## 1. Architectural & Module Patterns

### Hexagonal Crate Boundaries & Dependency Injection

- Follow the three-crate boundary strictly (see [`docs/architecture.md`](../../docs/architecture.md)):
  - `domain` and `application` contain pure business and orchestration logic (zero SQL, HTTP, or filesystem I/O).
  - Repository ports (`trait DocumentInstanceRepository`, `trait RoleRepository`, etc.) are defined in `domain` and implemented only in `infrastructure`.
- **Dependency Injection**: Inject port implementations via generics (static dispatch) or `Arc<dyn Trait>` (dynamic dispatch), keeping use-cases decoupled and testable with in-memory fakes.

```rust
trait Mailer { fn send(&self, to: &str, body: &str); }
struct Service<M: Mailer> { mailer: M }
```

### Facade (`pub use` & `pub(crate)`)

Expose a small, cohesive public API over internal submodules using `pub use`, while hiding worker submodules with private `mod` or `pub(crate) mod`:
- Prefer `pub(crate)` or private `mod` over `pub` for intra-crate items; only expose what crosses module or crate boundaries.
- A subsystem `mod.rs` (e.g., `persistence::migration`, `persistence::naming`, `persistence::repositories`) should make it immediately obvious which types/functions are public entry points and which are internal workers.

### Anti-Corruption Layer (ACL) & Serialization

- Isolate external wire formats (JSON schemas, REST payloads, database rows) from domain entities.
- Use `serde` with `#[serde(rename_all = "camelCase")]` and `validator` on API/config-facing DTOs in `infrastructure`.
- Do not bleed wire serialization rules into `domain` entities when the external representation differs.
- Use `serde_json::Value` at infrastructure boundaries for dynamic schema-driven payloads, converting to/from strongly-typed domain values (`ContentValue`, `DomainValue`) in dedicated codecs/mappers.

### Code Organization & Newspaper Item Ordering

Every Rust source file follows top-down dependency ordering ("newspaper style"):
1. **Imports**: Group as `std` → external crates → internal crates (`domain`, `application`, `crate::...`), separated by blank lines.
2. **Types First**: Constants, statics, type aliases, structs, and enums before `impl`s or functions.
3. **Implementations**: Inherent `impl`s first, trait `impl`s after.
4. **Functions**: Public functions first in dependency order (high-level orchestrators first, subroutines after); private helpers follow.
5. **Tests**: `#[cfg(test)] mod tests { ... }` at the bottom of the file.

### Clean Code & Function Design

Strictly follow [`.ai/skills/clean-code.md`](clean-code.md):
- **SRP**: One responsibility and one reason to change per function/struct.
- **SLAP**: High-level orchestrators coordinate steps and delegate raw I/O, parsing, SQL construction, and loops to focused helper functions.
- **Size & Nesting**: Target 10–30 lines per function (flag > 40 lines), max nesting depth 2, guard clauses (`?`) fail fast at the top.

---

## 2. Domain-Driven Design (DDD) Patterns

### Value Objects & Validated Newtypes (`nutype` & Structural Newtypes)

Wrap primitives to enforce invariants at construction time, prevent argument mix-ups, and implement foreign traits.

#### A. Domain Value Objects (`nutype`)

**Use [`nutype`](https://docs.rs/nutype) for all domain value objects.** Construction validates invariants so value objects are always valid by construction:

```rust
use nutype::nutype;

// Validated string identifier / value object
#[nutype(
    sanitize(trim),
    validate(not_empty, max_len = 255, regex = r"^[a-z][a-z0-9-]*$"),
    derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)
)]
pub struct DocumentTypeApiId(String);

// Validated numeric range
#[nutype(
    validate(greater = 0, less_or_equal = 65535),
    derive(Debug, Clone, Copy, PartialEq, Eq)
)]
pub struct IntegerPrecision(u8);
```

**Rules**:
- Every domain identifier (`DocumentTypeId`, `RelationId`, `AttributeId`, `UserId`, etc.) and constrained value object (`LocaleId`, `SlugValue`, `Email`, `Url`) **must** use `nutype` (or strongly typed UUID newtypes) — never pass raw `String`, `Uuid`, or `i64` in domain signatures.
- Identifier types live inside their owning subdomain (`domain/src/<subdomain>/`); shared value objects live in `domain/src/common/`.
- The `::try_new(raw)` constructor returns `Result<Self, NutypeError>` — always propagate with `?`.
- Do **not** use `nutype` in `infrastructure` wire DTOs — use plain types or zero-copy `&'a str` with `serde` / `validator` annotations and convert into domain newtypes.
- **Preferred `derive` set for Copy ID types**: `derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)`
- **Preferred `derive` set for string value objects**: `derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)`

#### B. Structural Newtypes

Use lightweight tuple structs when wrapping primitives for compile-time type safety or to satisfy Rust's orphan rule when implementing foreign traits:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticIden(pub &'static str);

impl sea_query::Iden for StaticIden {
    fn unquoted(&self) -> &str { self.0 }
}
```

### State Pattern (Enum State Machines)

Model lifecycle states as an enum (`PublicationState`, `DocumentStatus`, `AccessRequestStatus`), carrying state-specific data inside variants and modeling transitions through methods:

```rust
enum Conn { Idle, Open { id: u32 }, Closed }
impl Conn {
    fn open(self) -> Conn {
        match self { Conn::Idle => Conn::Open { id: 1 }, other => other }
    }
}
```

### Typestate Pattern

When invalid state transitions must be rejected at compile time rather than runtime, encode the state in a type parameter (`PhantomData<S>`):

```rust
struct Door<S>(std::marker::PhantomData<S>);
struct Open; struct Closed;
impl Door<Closed> { fn open(self) -> Door<Open> { Door(Default::default()) } }
impl Door<Open>   { fn close(self) -> Door<Closed> { Door(Default::default()) } }
// Door<Closed>.close() won't compile
```

### Command Pattern (Data Enums & Use-Case Commands)

Model commands and execution modes as plain data structs or enums (`ExecutionMode`, `CreateDocumentCommand`, `ApproveAccessRequestCommand`) so they are easy to inspect, validate, log, or dispatch:

```rust
enum Cmd { Insert(String), Delete(usize), Clear }
fn run(cmd: Cmd, data: &mut Vec<String>) {
    match cmd {
        Cmd::Insert(s) => data.push(s),
        Cmd::Delete(i) => { data.remove(i); }
        Cmd::Clear => data.clear(),
    }
}
```

### Domain Primitives: Primary Keys, Decimals & Deterministic Collections

- **Primary Keys (`Uuid::now_v7()`)**: Always use time-ordered `uuid::Uuid::now_v7()`. Never use auto-increment integers or database sequences (AWS Aurora DSQL does not support sequences).
- **Exact Decimals (`rust_decimal::Decimal`)**: Always use `Decimal` for financial amounts, schema numeric bounds, and precision numbers. Never use `f32` or `f64` in domain models.
- **Deterministic Collections (`IndexMap` / `IndexSet`)**: Use `indexmap::IndexMap` and `indexmap::IndexSet` whenever schema field order, attribute order, or insertion order must be preserved deterministically.
- **Zero-Allocation Lookups (`Borrow`)**: Implement `std::borrow::Borrow<IdType>` on domain definitions stored in `IndexSet` or `HashSet` to enable `set.get(&id)` without cloning.

---

## 3. Common GoF Patterns in Idiomatic Rust

### Creational Patterns

#### Builder

Construct complex objects, commands, or test fixtures (`DocumentTypeBuilder`, `FindDocumentsCommand`) step by step. Methods take and return `self`:

```rust
#[derive(Debug)]
struct Server { host: String, port: u16, tls: bool }

#[derive(Default)]
struct ServerBuilder { host: Option<String>, port: Option<u16>, tls: bool }

impl ServerBuilder {
    fn host(mut self, h: impl Into<String>) -> Self { self.host = Some(h.into()); self }
    fn port(mut self, p: u16) -> Self { self.port = Some(p); self }
    fn tls(mut self, t: bool) -> Self { self.tls = t; self }
    fn build(self) -> Result<Server, &'static str> {
        Ok(Server {
            host: self.host.ok_or("host required")?,
            port: self.port.unwrap_or(8080),
            tls: self.tls,
        })
    }
}
```

#### Factory (Associated Constructors & `From` / `TryFrom`)

Use associated functions (`::new`, `::try_new`, `::from_registry`) and standard conversion traits instead of separate factory classes:
- **Infallible Conversions**: Implement `From<T>` (which automatically provides `Into<T>`).
- **Explicit `From` over `.into()`**: In calling code, prefer `TargetType::from(x)` over `x.into()` when the target type is not immediately obvious or could become ambiguous during refactoring.
- **Fallible Conversions**: Implement `TryFrom<T>` returning a domain or crate error.

```rust
struct Port(u16);
impl TryFrom<u32> for Port {
    type Error = String;
    fn try_from(v: u32) -> Result<Self, String> {
        u16::try_from(v).map(Port).map_err(|_| format!("{v} out of range"))
    }
}
```

#### Singleton / Startup-Precomputed Static State

Never use `static mut`. Use `std::sync::OnceLock`, `std::sync::LazyLock`, or startup-precomputed immutable `&'static T` promoted once at the composition root (`SystemContext`, `SchemaNaming`):

```rust
use std::sync::OnceLock;
static CONFIG: OnceLock<String> = OnceLock::new();
fn config() -> &'static str { CONFIG.get_or_init(|| "prod".into()) }
```

### Structural Patterns

#### Adapter / Extension Trait

Add domain- or layer-specific helper methods to foreign types via a local extension trait:

```rust
trait StrExt { fn is_blank(&self) -> bool; }
impl StrExt for str {
    fn is_blank(&self) -> bool { self.trim().is_empty() }
}
```

#### Decorator (Generic Wrapper)

Wrap a trait implementor to layer cross-cutting concerns (logging, caching, metrics, retries):

```rust
trait Handler { fn handle(&self, req: &str) -> String; }

struct Logging<H: Handler>(H);
impl<H: Handler> Handler for Logging<H> {
    fn handle(&self, req: &str) -> String {
        println!("-> {req}");
        self.0.handle(req)
    }
}
```

#### Interior Mutability

Mutate through a shared reference when required by a trait signature (e.g., in-memory repository fakes in `test_support`):
- Single-threaded: `Rc<RefCell<T>>`
- Multi-threaded (`Send + Sync`): `Arc<Mutex<T>>` or `Arc<RwLock<T>>`

### Behavioral Patterns

#### Strategy

Pass pluggable behavior as a generic closure (`Fn` / `FnMut`), a generic trait bound, or a `dyn` trait (e.g., `with_occ_retry` wrapping any transactional closure):

```rust
fn apply<F: Fn(f64) -> f64>(x: f64, strategy: F) -> f64 { strategy(x) }
apply(100.0, |p| p * 0.9);
```

#### Observer (Channels)

Decouple event producers from consumers using `std::sync::mpsc` or `tokio::sync::{mpsc, broadcast, watch}`.

#### Iterator

Implement `Iterator` or return `impl Iterator<Item = ...>` to expose lazy, zero-allocation traversal with standard combinators (`map`, `filter`, `collect`).

#### Visitor (Exhaustive `match`)

For closed type hierarchies (`FieldType`, `DomainValue`, `MigrationStep`), prefer exhaustive `match` expressions so the compiler enforces handling of new variants. Use a visitor trait only for open hierarchies.

---

## 4. Rust-Specific Idioms, Error Handling & Async Runtime

### Error Enum with `?` (`thiserror`)

- Use `thiserror` for all library error types; define one cohesive `Error` enum per crate/subsystem (`DomainError`, `ApplicationError`, `PersistenceError`, `CliError`).
- Propagate errors with `?` and automatic `#[from]` / `From` conversions or `.map_err(...)`.
- **Never use `.unwrap()` or `.expect()` in non-test code.**
- In `infrastructure`, centralize the translation of `ApplicationError` / `DomainError` into HTTP status codes and RFC 9457 `ProblemDetails`.

```rust
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("parse: {0}")] Parse(#[from] std::num::ParseIntError),
}
```

### RAII / Drop Guard

Release resources or clean up temporary state deterministically when a guard goes out of scope via `Drop`.

### Sealed Trait

Prevent downstream crates from implementing a fixed public trait by requiring a private supertrait:

```rust
mod sealed { pub trait Sealed {} }
pub trait Format: sealed::Sealed {}
impl sealed::Sealed for Json {}
impl Format for Json {}
pub struct Json;
```

### Async & Observability (`tokio` & `tracing`)

- Runtime: `tokio` (multi-thread).
- Annotate async service/repository entry points with `#[instrument]` (`tracing`) for structured observability.
- Avoid `tokio::spawn` in `domain` or `application` layers — task spawning and background concurrency belong in `infrastructure`.

---

## 5. Preferred Crates, Testing, Clippy & Anti-Patterns

### Preferred Crates

| Purpose | Crate |
|---|---|
| HTTP framework | `axum` |
| Database & SQL AST | `sqlx` (PostgreSQL), `sea-query` |
| Async runtime | `tokio` |
| Serialization | `serde`, `serde_json` |
| UUIDs | `uuid` (`v7` feature) |
| Dates & timestamps | `chrono` |
| Exact decimals | `rust_decimal` |
| Ordered maps/sets | `indexmap` |
| Errors | `thiserror` (libraries), `anyhow` (bins/tests only) |
| Tracing | `tracing`, `tracing-subscriber` |
| Validation & Newtypes | `nutype` (domain value objects), `validator` (infra DTOs) |

### Testing

- Place inline unit tests in `#[cfg(test)] mod tests` at the bottom of the module under test.
- Use `#[tokio::test]` for async tests.
- Follow [`.ai/skills/testing.md`](testing.md) and [`.ai/skills/spec-testing.md`](spec-testing.md) for integration and spec-traceable tests.

### Clippy & Anti-Patterns to Avoid

All code must pass `cargo clippy --workspace --all-targets -- -D warnings`.

- **No `Deref` for inheritance**: Never simulate subclassing with `Deref`; use composition and explicit traits or delegation methods.
- **No `.clone()` to silence the borrow checker**: Rethink ownership first — pass borrowed slices (`&str`, `&[T]`), move owned values out of collections with `.remove(&k)` when consuming, or use `Arc` / startup `&'static` deliberately.
- **No `#![deny(warnings)]` in library crates**: Enforce `-D warnings` via `cargo clippy` in CI/workflows so new compiler lints do not break builds prematurely.
- **Avoid common clippy pitfalls**: Unused `mut`, needless borrows (`&*foo` → `foo.as_ref()`), single-arm `match` where `if let` is clearer, and unchecked `as` integer casts (`u32 as i32` → `i32::try_from(...)`).
