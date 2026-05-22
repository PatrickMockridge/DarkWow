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

To find outstanding tasks, grep for inline TODO/FIXME markers:
```
$ git grep -E 'TODO|FIXME'
```

## Troubleshooting

The `linear-master` branch is considered bleeding-edge so stability issues can occur.
If you encounter issues, try the steps below. It is a good idea to revisit these steps
periodically as things change.

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

If you are able to trigger a crash, report it via the [security disclosure](#security-disclosure) process.

Further information on fuzzing in DarkWow is available in the `fuzz/` directory.

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

## Security Disclosure

Report vulnerabilities privately via Codeberg:

1. Go to the [Security](https://codeberg.org/PatrickM123/darkwow/security) tab
2. Use **"Report a vulnerability"** to submit details privately

This notifies the maintainers without disclosing the issue publicly. Expect a
response within 48 hours.
