# Design Patterns in Rust

Many classic GoF patterns shrink or disappear in Rust because of enums, traits, closures, and ownership. Below are the ones that matter, in idiomatic form.

## Creational

### Builder

Construct complex objects step by step. Methods take and return `self`.

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
// ServerBuilder::default().host("localhost").port(443).tls(true).build()
```

### Factory (constructors and `From`/`TryFrom`)

Use associated functions and conversion traits instead of factory classes.

```rust
struct Port(u16);
impl TryFrom<u32> for Port {
    type Error = String;
    fn try_from(v: u32) -> Result<Self, String> {
        u16::try_from(v).map(Port).map_err(|_| format!("{v} out of range"))
    }
}
```

### Singleton / Global State

Use `OnceLock` or `LazyLock`, not mutable statics.

```rust
use std::sync::OnceLock;
static CONFIG: OnceLock<String> = OnceLock::new();
fn config() -> &'static str { CONFIG.get_or_init(|| "prod".into()) }
```

## Structural

### Newtype

Wrap a primitive to get type safety and to implement foreign traits.

```rust
struct UserId(u64);
struct OrderId(u64);
fn load_user(id: UserId) { /* can't pass an OrderId by mistake */ }
```

### Adapter / Extension Trait

Add methods to types you don't own.

```rust
trait StrExt { fn is_blank(&self) -> bool; }
impl StrExt for str {
    fn is_blank(&self) -> bool { self.trim().is_empty() }
}
```

### Decorator (Generic Wrapper)

Wrap a trait implementor and add behavior.

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

### Facade

A small public API over several internal modules, using `pub use` and `pub(crate)` to hide the rest.

### Interior Mutability

Mutate through a shared reference, with the check moved to runtime.

```rust
use std::{cell::RefCell, rc::Rc};
let shared = Rc::new(RefCell::new(vec![1, 2]));
shared.borrow_mut().push(3);
// multi-threaded: Arc<Mutex<T>> or Arc<RwLock<T>>
```

## Behavioral

### Strategy

Pass behavior as a generic trait bound, a `dyn` trait, or just a closure.

```rust
fn apply<F: Fn(f64) -> f64>(x: f64, strategy: F) -> f64 { strategy(x) }
apply(100.0, |p| p * 0.9); // 10% discount
```

### State (Enum)

Model states as an enum, with transitions as methods that consume `self`.

```rust
enum Conn { Idle, Open { id: u32 }, Closed }
impl Conn {
    fn open(self) -> Conn {
        match self { Conn::Idle => Conn::Open { id: 1 }, other => other }
    }
}
```

### Command (Enum)

Commands are data, so they are easy to queue, log, or send over channels.

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

### Observer (Channels)

Decouple producers from consumers with `mpsc` or `tokio::sync::broadcast`.

```rust
use std::sync::mpsc;
let (tx, rx) = mpsc::channel();
std::thread::spawn(move || tx.send("event").unwrap());
println!("{}", rx.recv().unwrap());
```

### Iterator

Implement the `Iterator` trait and get `map`, `filter`, `collect`, and so on for free.

```rust
struct Countdown(u32);
impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 { None } else { self.0 -= 1; Some(self.0 + 1) }
    }
}
```

### Visitor

Usually replaced by `match` on an enum. For open hierarchies, use a visitor trait.

```rust
enum Expr { Num(f64), Add(Box<Expr>, Box<Expr>) }
fn eval(e: &Expr) -> f64 {
    match e {
        Expr::Num(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
    }
}
```

## Rust-Specific Idioms

### RAII / Drop Guard

Release resources deterministically when a value goes out of scope.

```rust
struct Timer(std::time::Instant);
impl Drop for Timer {
    fn drop(&mut self) { println!("took {:?}", self.0.elapsed()); }
}
// let _t = Timer(Instant::now()); // prints when scope ends
```

### Typestate

Encode state in the type so invalid transitions fail at compile time.

```rust
struct Door<S>(std::marker::PhantomData<S>);
struct Open; struct Closed;
impl Door<Closed> { fn open(self) -> Door<Open> { Door(Default::default()) } }
impl Door<Open>   { fn close(self) -> Door<Closed> { Door(Default::default()) } }
// Door<Closed>.close() won't compile
```

### Sealed Trait

Stop downstream crates from implementing your public trait.

```rust
mod sealed { pub trait Sealed {} }
pub trait Format: sealed::Sealed {}
impl sealed::Sealed for Json {}
impl Format for Json {}
pub struct Json;
```

### Error Enum with `?`

Usually via `thiserror`: one error type per layer, with automatic conversion.

```rust
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("parse: {0}")] Parse(#[from] std::num::ParseIntError),
}
fn read_num(p: &str) -> Result<i32, AppError> {
    Ok(std::fs::read_to_string(p)?.trim().parse()?)
}
```

### Dependency Injection

Take traits via generics (static dispatch) or `Box<dyn Trait>` / `Arc<dyn Trait>` (dynamic), which is also what makes mocking easy.

```rust
trait Mailer { fn send(&self, to: &str, body: &str); }
struct Service<M: Mailer> { mailer: M }
```

## Anti-Patterns to Avoid

- **`Deref` for inheritance**: simulating subclassing with `Deref` is confusing. Use composition and traits.
- **Clone to silence the borrow checker**: rethink ownership or use `Rc`/`Arc` deliberately.
- **`#![deny(warnings)]` in libraries**: it breaks builds when new compiler lints appear.