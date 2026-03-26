# Writing a daemon

DarkFi consists of many seperate daemons communicating with each other. To
run the p2p network, we'll need to implement our own daemon.  So we'll
start building `dchat` by creating a daemon that we call `dchatd`.

To do this, we'll make use of a DarkFi macro called
[async_daemonize](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/util/cli.rs).

`async_daemonize`is the standard way of daemonizing darkfi binaries. It
implements TOML config file configuration, argument parsing and a
multithreaded async executor that can be passed into the given function.

We use `async_daemonize` as follows:

```rust
use darkfi::{async_daemonize, cli_desc, Result};
use smol::stream::StreamExt;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

const CONFIG_FILE: &str = "dchatd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dchatd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "daemond", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    println!("Hello, world!");
    Ok(())
}
```

Behind the scenes, `async_daemonize` uses `structopt` and `structopt_toml`
crates to build command line arguments as a struct called `Args`. It spins
up a `smol::Executor` for async task management, and implements signal handling
to properly terminate the daemon on receipt of a stop signal.

> **How async_daemonize! works**: Under the hood, the macro expands to create
> an `Arc<smol::Executor<'static>>` for shared task spawning, uses
> `async_channel::bounded(1)` for shutdown signal communication, and calls
> `smol::block_on()` to bridge synchronous and asynchronous code.
>
> See [Async Rust in Practice: The DarkFi Experience (Part 6)](https://technologytruth.substack.com/p/async-rust-in-practice-the-darkfi-a6a) for a detailed macro expansion walkthrough.

`async_daemonize` allow us to spawn the config data we specify at
`CONFIG_FILE_CONTENTS` into a directory either specified using the
command-line flag `--config`, or in the default darkfi config directory.

`async_daemonize` also implements logging that will output
different levels of debug info to the terminal, or to both the terminal
and a log file if a log file is specified.
