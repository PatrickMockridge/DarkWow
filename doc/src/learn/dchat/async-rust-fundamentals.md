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

**Reference**: [Rust for Rustaceans (RfR) - Pinning](https://rustforrustaceans.com/rocking/pinning)

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

**Reference**: [TRPL - Mutex](https://doc.rust-lang.org/book/ch16-03-shared-state.html#mutex)

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

**Reference**: [RfR - Executors](https://rustforrustaceans.com/rocking/async)

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

## StoppableTask

DarkFi's `StoppableTask` manages long-running async tasks with graceful
shutdown. It wraps a future and provides a `stop()` method to signal
termination:

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
completion or stop.

**Reference**: See [StoppableTask](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/system/stoppable_task.rs)

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
