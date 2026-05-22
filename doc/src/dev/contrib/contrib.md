# Contributing Developer Guide

## Fastest way to get started

1. **Clone and build** (5 minutes):
   ```
   git clone https://codeberg.org/PatrickM123/darkwow
   cd darkwow
   rustup target add wasm32-unknown-unknown
   make
   ```

2. **Run the lightweight test pipeline** (30 seconds):
   ```
   cargo test -p dwowd test_pipeline
   ```

3. **Run a local devnet** (requires Docker, 2 minutes):
   ```
   cd contrib/docker/darkwow-testnet
   docker compose up -d
   docker compose logs -f
   ```

4. **Explore the codebase**: See the [Developer Quick Start Guide](../quickstart.md)
   for the full four-level testing taxonomy and architecture tour.

Minimum Rust version: **1.87.0**. Builds on Linux (x86_64, aarch64) and macOS.

## Community

Every Monday 14:00 UTC (DST) or 15:00 UTC (ST), there is a dev
meeting on [DarkIRC](../../misc/darkirc/darkirc.md). Feel free to join
and discuss with other DarkWow devs.

Contribute according to your own interests, skills, and topics in which you would
like to become more knowledgeable. Take initiative. Other DarkWow devs can help you
as mentors: see [the Methodology section of the Study Guide](../../philosophy/learn.md#methodology).

Few people are able be an expert in all domains. Choose a topic and specialize.
Example specializations are described [here](../../philosophy/learn.md#branches).
Don't make the mistake that you must become an expert in all areas before getting started.
It's best to just jump in.

## Finding specific tasks

Tasks are usually noted in-line using code comments. All of these tasks should be resolved
and can be considered a priority.

To find them, run the following command:
```
$ git grep -E 'TODO|FIXME'
```

## Areas of work

There are several areas of work that are either undergoing maintenance 
or need to be maintained:

* **Documentation:** general documentation and code docs (cargo doc).
* **TODO** and **FIXME** are throughout the codebase. Find your favourite one and begin hacking.
* **Tooling:** Creating new tools or improving existing ones.
    * Improve the ZK tooling. For example tools to work with txs, smart contracts and ZK proofs.
    * Also document zkrunner and other tools.
* **Tests:** Throughout the project there are either broken or commented out unit tests, they need to be fixed.
* Harder **crypto** tasks:
    * MoneyV3::transfer() contract viewing keys

## Fuzz testing

Fuzz testing is a method to find important bugs in software. It becomes more 
powerful as more computing power is allocated to it. 

You can help to test DarkWow by running our fuzz tests on your machine. No
specialized hardware is required. 

As fuzz testing benefits from additional CPU power, a good method for running
the fuzzer is to let it run overnight or when you are otherwise not using
your device.

### Set-up
After running the normal commands to set-up DarkWow as described in the README, run the following commands.

```
# Install cargo fuzz
$ cargo install cargo-fuzz
```

Run the following from the DarkWow repo folder:

```
$ cd fuzz/
$ cargo +nightly fuzz list
```

This will list the available fuzzing targets. Choose one and run it with:

### Run
```
# format: cargo +nightly fuzz run TARGET
# e.g. if `serial` is your target:
$ cargo +nightly fuzz run --all-features -s none --jobs $(nproc) serial 
```

This process will run infinitely until a crash occurs or until it is cancelled by the user.

If you are able to trigger a crash, get in touch with the DarkWow team via irc.

Further information on fuzzing in DarkWow is available in the `fuzz/` directory.

## Troubleshooting

The `linear-master` branch is considered bleeding-edge so stability issues can occur.
If you encounter issues, try the steps below. It is a good idea to revisit these steps
periodically as things change. For example, even if you have already installed all
dependencies, new ones may have been recently added and this could break your
development environment.

* Clear out artifacts and get a fresh build environment: 

```sh
# Get to the latest commit
$ git pull
# Clean build artifacts
$ make distclean
```

* Remove `Cargo.lock`. This will cause Rust to re-evaluate dependencies and could help
if there is a version mismatch.

* Ensure all dependencies are installed. Check the README.md and/or run:

```
$ sh contrib/dependency_setup.sh
```

* Ensure that you are building for `wasm32-unknown-unknown`.
Check `README.md` for instructions.

* When running a `cargo` command, use the flag `--all-features`.

## Commit messages

If your commit is changing a specific module in the code and not
touching other parts of the codebase, write a commit message that
mentions which module was changed. For example:

> `crypto/keypair: added foo method for Bar struct.`

Use the commit body to explain your intentions.

## Code style

Run `make fmt` before committing. You can enforce this with a git
`pre-commit` hook:

```shell
#!/bin/sh
if ! cargo +nightly fmt --all -- --check >/dev/null; then
    echo "There are some code style issues. Run 'make fmt' on repo root to fix it."
    exit 1
fi
exit 0
```

Place this script in `.git/hooks/pre-commit` and make it executable.

## Testing crate features

The library heavily depends on cargo features. Use
[`cargo hack`](https://github.com/taiki-e/cargo-hack) to check all
feature combinations compile. Install it and run `make check`.

## Code coverage

Run codecov tests using
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```
$ cargo install cargo-llvm-cov
$ make coverage
```

Reports are in `target/llvm-cov/html/index.html`.

## Static binary builds

Using musl-libc we can produce statically linked binaries.

### Using LXC (Alpine)

```sh
# lxc-create -n xbuild-alpine -t alpine -- --release edge
# lxc-start -n xbuild-alpine
# lxc-attach -n xbuild-alpine
```

Inside the container:

```sh
# apk add rustup git musl-dev make gcc openssl-dev openssl-libs-static tcl-dev zlib-static
# wget -O sqlcipher.tar.gz https://github.com/sqlcipher/sqlcipher/archive/refs/tags/v4.5.5.tar.gz
# tar xf sqlcipher.tar.gz
# cd sqlcipher-4.5.5
# ./configure --prefix=/usr/local --disable-shared --enable-static
# make -j$(nproc) && make install
# cd ~
# rustup-init --default-toolchain stable -y
# source ~/.cargo/env
# rustup target add wasm32-unknown-unknown --toolchain stable
# git clone https://codeberg.org/PatrickM123/darkwow -b linear-master --depth 1
# cd darkwow
# make darkirc
```

### Native musl

```sh
$ rustup target add x86_64-unknown-linux-musl --toolchain stable
$ make RUST_TARGET=x86_64-unknown-linux-musl darkirc
```

## Security Disclosure

Join our DarkIRC chat and ask to speak with the core team.

Usually the best time would be our weekly Monday meetings at 14:00 UTC
(DST) or 15.00 UTC (ST).

If it's sensitive and time critical, then we will get in touch over DM,
and we will post a message on darkwow.org to confirm our identity once we're in
contact over DM.

We haven't yet clarified our bug bounty program (stay tuned), but for legit bug
reports we will pay out fairly.

