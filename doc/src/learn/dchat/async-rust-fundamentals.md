# Async Rust Fundamentals

This preamble covers the async Rust concepts essential for building P2P
applications on DarkFi. Familiarity with these patterns is assumed for
the rest of the tutorial.

## Why Async?

Synchronous code blocks on I/O operations. In a P2P network where your
program waits on many concurrent connections, blocking would be
prohibitively expensive. Async Rust allows you to run thousands of
concurrent operations on a small number of threads by yielding CPU
control when waiting on I/O.

**Reference**: [The Rust Programming Language (TRPL) - Async](https://doc.rust-lang.org/book/ch17-02-concurrency.html)

## Runtime Choice: smol over Tokio

DarkFi uses the **smol** runtime (~3,000 lines) rather than Tokio. This
minimalist approach prioritizes:

- **Auditability**: A small, readable codebase is crucial for security-critical blockchain software
- **Predictability**: Minimal abstractions mean fewer surprises
- **Correctness**: Less complexity means fewer hidden bugs

smol provides only the essentials: executor, basic I/O, timers, spawning,
and minimal sync primitives. Every macro expands to plain Rust code that
rigorously follows the rules of ownership, borrowing, lifetimes, generics,
and traits.

**Further reading**:
- [Async Rust in Practice: The DarkFi Experience (Part 1)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi)
- [Async Rust in Practice: The DarkFi Experience (Part 2)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi-3f9)

## async/await

The `async` keyword creates a value representing a computation that may
complete in the future. The `.await` operator suspends the current
task until the future completes:

```rust
async fn fetch_data() {
    let future = some_async_operation();
    let result = future.await; // Suspends until ready
}
```

**Reference**: [TRPL - async/await](https://doc.rust-lang.org/book/ch17-02-concurrency.html#async-await)

## async_daemonize! Macro

DarkFi provides the `async_daemonize!` macro which encapsulates complex
daemon initialization:

```rust
async_daemonize!(real_main);
```

This macro handles:
- Config parsing from CLI arguments
- Logging setup with tracing
- Executor creation
- Signal handling (SIGINT/SIGTERM)
- Spawning the main async task

Under the hood it uses:
- `smol::Executor` for async task management
- `smol::block_on()` to bridge sync and async code
- `async_channel::bounded(1)` for shutdown signal communication
- `Arc<smol::Executor<'static>>` for shared task spawning

**Further reading**:
- [Async Rust in Practice: The DarkFi Experience (Part 6)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi-a6a)

## Pinning

`async` blocks create self-referential structs that cannot be moved in
memory. Pinning fixes an object in memory so its address remains
stable. You'll encounter `Pin<Arc<T>>` frequently when passing
protocols between async tasks:

```rust
use std::pin::Pin;
use std::sync::Arc;

let pinned: Pin<Arc<ProtocolDchat>> = Arc::new(protocol).into();
```

**Reference**: [RfR - Pinning](https://rustforrustaceans.com/rocking/pinning)

## Arc: Atomic Reference Counting

`Arc<T>` is a thread-safe reference-counting pointer. Multiple parts of
your program can own a reference to the same data, and the data is
freed when the last reference is dropped:

```rust
use std::sync::Arc;

let shared_data: Arc<Mutex<Vec<DchatMsg>>> = Arc::new(Mutex::new(vec![]));
let data_clone = shared_data.clone(); // Reference count increments
```

In dchat, `Arc` is used extensively to share the message buffer between
the protocol handler and the RPC server.

### Breaking Reference Cycles with Weak

When a parent owns a child but the child needs to reference the parent,
using `Arc` would create a reference cycle (memory leak).
`Weak<P2p>` solves this by allowing temporary references without ownership:

```rust
struct P2p {
    channels: HashMap<ChannelId, Weak<Channel>>,
}
```

The parent (`P2p`) owns the children (`Channel`), but channels can
temporarily access the P2p without preventing it from being dropped.

### Arc::new_cyclic for Self-Referential Structures

Sometimes you need a struct that references itself. `Arc::new_cyclic`
creates `Arc<T>` where `T` contains `Weak<T>`:

```rust
let p2p: P2pPtr = Arc::new_cyclic(|weak_self| {
    P2p { channels: HashMap::new(), weak_self: weak_self.clone() }
});
```

**Reference**: [TRPL - Reference Cycles](https://doc.rust-lang.org/book/ch15-04-rc.html#reference-cycles)

## Mutex: Synchronization

A `Mutex` (mutual exclusion) protects shared data by ensuring only one
task can access it at a time. DarkFi uses `smol::lock::Mutex` (a
futures-aware mutex) rather than `std::sync::Mutex` to avoid blocking
the thread during async operations:

```rust
use smol::lock::Mutex;

let buffer: Arc<Mutex<Vec<DchatMsg>>> = Arc::new(Mutex::new(vec![]));
let guard = buffer.lock().await; // Async lock acquisition
guard.push(message);
drop(guard); // Lock released
```

**Key rule**: Never hold a `std::sync::Mutex` guard across an `.await`
point — use `smol::lock::Mutex` instead. Holding a sync mutex across
an await point blocks the thread and can deadlock.

## RwLock: Read-Write Locking

For data that's read more often than written, `RwLock` allows multiple
concurrent readers or a single exclusive writer:

```rust
use smol::lock::RwLock;

let state: Arc<RwLock<HashMap<String, Session>>> = Arc::new(RwLock::new(HashMap::new()));

// Multiple readers can proceed concurrently
let readers = state.read().await;
```

## async_trait

Traits with async methods require the `async_trait` crate. The macro
desugars async methods into synchronous methods returning `Pin<Arc<dyn Future>>`:

```rust
use async_trait::async_trait;

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        // ...
    }
}
```

**Reference**: [RfR - Async Traits](https://rustforrustaceans.com/rocking/async)

## Executors

An executor runs async code. DarkFi uses `smol::Executor` to run async
tasks on a thread pool. When you call `.await` on a future, you're
yielding control to the executor:

```rust
use smol::Executor;

let ex = Arc::new(Executor::new()?);
ex.run(async {
    // Async code runs here
}).await;
```

Protocols receive the executor via their `start()` method to spawn
subtasks:

```rust
self.jobsman.clone().spawn(
    self.clone().handle_receive_msg(),
    executor.clone()
).await;
```

**Further reading**:
- [Async Rust in Practice: The DarkFi Experience (Part 3)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi-e16)

## Streams

Streams are like async iterators—they produce values over time. The
P2P network receives messages as streams:

```rust
use smol::stream::StreamExt;

while let Ok(msg) = self.msg_sub.receive().await {
    // Process message
}
```

`StreamExt` provides methods like `next()`, `for_each()`, and
`filter()` for composing stream operations.

**Reference**: [RfR - Streams](https://rustforrustaceans.com/rocking/async)

## StoppableTask: Cooperative Shutdown

DarkFi's `StoppableTask` manages long-running async tasks with graceful
shutdown. It uses a `watch` channel to signal tasks cooperatively,
avoiding the risks of forceful cancellation:

```rust
use darkfi::system::StoppableTask;

let task = StoppableTask::new();
task.clone().start(
    async move { loop { /* work */ } },
    |res| async { /* cleanup */ },
    Error::DetachedTaskStopped,
    ex.clone(),
);

// Later, to stop:
task.stop().await;
```

The second argument is a callback that handles the task's result on
completion or stop. Tasks periodically check `should_stop()` and exit
gracefully on signal.

**Why cooperative shutdown?**
- Forceful `abort` can leave resources in bad states
- Cooperative shutdown lets tasks clean up properly
- Watch channels transfer ownership of the stop signal

**Further reading**:
- [Async Rust in Practice: The DarkFi Experience (Part 4)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi-d66)

## Protocol and Message Traits

DarkFi's P2P networking is built on two core traits:

### Message Trait

Messages are network-serializable types with metadata:

```rust
pub trait Message: Serialize + DeserializeOwned + Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> u32;
}
```

### ProtocolBase Trait

All protocols implement this standard interface:

```rust
#[async_trait]
pub trait ProtocolBase: Send + Sync {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()>;
    fn name(&self) -> &'static str;
}
```

Protocols typically:
1. Subscribe to message types via `channel.subscribe_msg::<T>()`
2. Spawn async tasks via `ProtocolJobsManager`
3. Use `Arc<Mutex<T>>` for shared state

**Further reading**:
- [Async Rust in Practice: The DarkFi Experience (Part 5)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi-e16)

## Ownership: The Foundation

Every concept above builds on Rust's core ownership model:

| Concept | Rust Foundation |
|---------|-----------------|
| `Arc<T>` | Reference counting (ownership shared) |
| `Mutex<T>` | Aliasing rules (`&mut` exclusive) |
| `Pin<Arc<T>>` | Self-referential structs (pinned memory) |
| `Weak<T>` | Breaking reference cycles (non-owning reference) |
| `async move` | Ownership transfer into futures |

Understanding ownership and borrowing makes the compiler predictable
rather than opaque. Features like async/await, concurrency, and smart
pointers are "sugar" over these same principles.

**Further reading**:
- [Pareto-Efficient Rust: Understanding Ownership and Borrowing](https://technologytruth.substack.com/p/pareto-efficient-rust-understanding)

## Common Patterns in dchat

### Cloning Arc pointers

When spawning tasks that need access to shared state, clone the `Arc`:

```rust
let msgs_ = msgs.clone(); // Increments reference count
registry.register(SESSION_DEFAULT, move |channel, _p2p| {
    async move { ProtocolDchat::init(channel, msgs_).await }
}).await;
```

### Channel passing

Channels are passed to protocol constructors and contain the connection
to a remote peer. The protocol subscribes to message types through the
channel:

```rust
let msg_sub = channel.subscribe_msg::<DchatMsg>().await.expect("Missing dispatcher!");
```

### Error handling

DarkFi uses a custom `Error` type. Protocol methods return `Result<()>`
which is a type alias for `Result<(), Error>`. Use the `?` operator
or `match` for error handling:

```rust
async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
    while let Ok(msg) = self.msg_sub.receive().await {
        // ...
    }
    Ok(())
}
```

## Further Reading

* [The Rust Programming Language](https://doc.rust-lang.org/book/) - Chapters 15-17
* [Rust for Rustaceans](https://rustforrustaceans.com/) - "Rocking" section on async
* [Asynchronous Programming in Rust](https://rust-lang.github.io/async-book/) - Official async book
* [Async Rust in Practice: The DarkFi Experience (substack series)](https://technologytruth.substack.com)
