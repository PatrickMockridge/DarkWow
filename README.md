# DarkFi - Anonymous, Uncensored, Sovereign

![Build Status](https://img.shields.io/github/actions/workflow/status/darkrenaissance/darkfi/ci.yml?branch=master&style=flat-square)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white&style=flat-square)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white&style=flat-square)](https://dark.fi/manifesto.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white&style=flat-square)](https://dark.fi/book/)

## Development Fork

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

This is a development fork of the official DarkFi repository. **Development occurs on the `master` branch** (`PatrickM123/darkfi:master`).

This fork contains all additions compared to official DarkFi master:

- **New smart contracts**: Bridge, DEX, Identity, Stablecoin, Escrow, DAO-Escrow, Subscription, Atomic Swap, Labor Market, Auction, Tender, Attestation, Oracle (not in official DarkFi)
- **Stablecoin refactor**: Synthetix-style pooled debt model (replacing individual CDPs)
- **Identity refactor**: Level 0 (zk_only) with safemath assertion gadgets
- **Expanded architecture documentation**: Additional analysis and design approaches including [Subscription](doc/src/arch/subscription.md), [Atomic Swap](doc/src/arch/atomic_swap.md), [Labor Market](doc/src/arch/labor_market.md), [Auction](doc/src/arch/auction.md), [Tender](doc/src/arch/tender.md), [Attestation](doc/src/arch/attestation.md), and [Oracle](doc/src/arch/oracle.md) contracts
- **Architecture reference docs**: [Experimental Opcodes](doc/src/arch/experimental-opcodes.md) (grey-market opcode analysis), [Merkle Depth](doc/src/arch/merkle_depth.md) (fixed-depth limitations and workarounds), [Composability](doc/src/arch/composability.md) (smart contract composition patterns with real-world DAO failure analysis and Tender + Labor Market + Attestation integration), [Safemath](doc/src/arch/safemath.md) (ZK arithmetic templates as LessThanOrEqual workaround)
- **Work-in-progress implementations**: Skeleton code and alternative approaches

**Key new features:**
- **Attestation contract**: Generalized attestation and claims system for reusable verification patterns
- **Oracle contract**: Push-model oracle demonstrating external data integration via attestation
- **Safemath integration**: Production-ready assertion gadgets for bounded arithmetic (workaround for LessThanOrEqual gate soundness issue)
- **Pooled debt stablecoin**: Synthetix-style shared liability model replacing individual CDPs

**Security Status**: All new contracts are EXPERIMENTAL and UNAUDITED. Known security issues are documented in [Security Analysis](doc/src/arch/security-analysis.md). The official DarkFi repository should be consulted for the canonical, production-ready state.

---



We aim to proliferate [anonymous digital
markets](https://dark.fi/manifesto.html) by means of strong cryptography
and peer-to-peer networks. We are establishing an online zone of freedom
that is resistant to the surveillance state.

> Unfortunately, the law hasn’t kept pace with technology, and this disconnect
> has created a significant public safety problem. We call it "Going Dark".
>
> James Comey, FBI director

So let there be dark.

## About DarkFi

DarkFi is a new Layer 1 blockchain, designed with anonymity at the
forefront. It offers flexible private primitives that can be wielded
to create any kind of application. DarkFi aims to make anonymous
engineering highly accessible to developers.

DarkFi uses advances in zero-knowledge cryptography and includes a
contracting language and developer toolkits to create uncensorable
code.

In the open air of a fully dark, anonymous system, cryptocurrency has
the potential to birth new technological concepts centered around
sovereignty. This can be a creative, regenerative space - the dawn of
a Dark Renaissance.

## Connect to DarkFi Alpha Testnet

DarkFi Alpha Testnet is a PoW blockchain that provides fully anonymous
transactions, zero-knowledge contracts, anonymous atomic swaps, a
self-governing anonymous DAO, and more.

- `darkfid` is the DarkFi fullnode. It validates blockchain
transactions and stays connected to the p2p network.
- `drk` is a CLI wallet. It provides an interface to smart contracts
such as Money and DAO, manages our keys and coins, and scans the
blockchain to update our balances.
- `xmrig` is the mining daemon used in DarkFi. Connects to `darkfid`
over its `Stratum` RPC, and requests new block headers to mine.

To connect to the alpha testnet, [follow the tutorial][tutorial].

[tutorial]: https://dark.fi/book/testnet/node.html

## Connect to DarkFi IRC

Follow the [installation instructions][darkirc-instructions] for the
P2P IRC daemon.

[darkirc-instructions]: https://dark.fi/book/misc/darkirc/darkirc.html#installation

## Build

First you need to clone DarkFi repo and enter its root folder, if
you haven't already done it:

```shell
% git clone https://codeberg.org/PatrickM123/darkfi
% cd darkfi
```

This project requires the Rust compiler to be installed. 
Please visit [Rustup](https://rustup.rs/) for instructions.

You have to install a native toolchain, which is set up during Rust installation,
and wasm32 target.
To install wasm32 target, execute:

```shell
% rustup target add wasm32-unknown-unknown
```
Minimum Rust version supported is **1.87.0**.

The following dependencies are also required:

|   Dependency   |   Debian-based     |
|----------------|--------------------|
| git            | git                |
| cmake          | cmake              |
| make           | make               |
| gcc            | gcc                |
| g++            | g++                |
| pkg-config     | pkg-config         |
| alsa-lib       | libasound2-dev     |
| clang          | libclang-dev       |
| fontconfig     | libfontconfig1-dev |
| lzma           | liblzma-dev        |
| openssl        | libssl-dev         |
| sqlcipher      | libsqlcipher-dev   |
| sqlite3        | libsqlite3-dev     |

Users of Debian-based systems (e.g. Ubuntu) can simply run the
following to install the required dependencies:

```shell
# apt-get update
# apt-get install -y git cmake make gcc g++ pkg-config libasound2-dev libclang-dev libfontconfig1-dev liblzma-dev libssl-dev libsqlcipher-dev libsqlite3-dev
```

Alternatively, users can try using the automated script under `contrib`
folder by executing:

```shell
% sh contrib/dependency_setup.sh
```

The script will try to recognize which system you are running,
and install dependencies accordingly. In case it does not find your
package manager, please consider adding support for it into the script
and sending a patch.

Lastly, we can build the necessary binaries using the provided
Makefile, to build the project. If you want to build specific ones,
like `darkfid` or `darkirc`, skip this step, as it will build
everything, and use their specific targets instead.

```shell
% make
```

## Development

If you want to hack on the source code, make sure to read some
introductory advice in the
[DarkFi book](https://dark.fi/book/dev/dev.html).

## Reference Materials

**Uncensorable ZK and DarkFi Reference Material (Arweave)**

DarkFi Reference Material stored permanently on Arweave:
- [DarkFi Reference Material](https://app.ardrive.io/#/drives/f79597cd-8a4e-426e-840e-25c1453e418d?name=DarkFi+Reference+Material) - Textbooks, papers, and materials on ZK circuits, cryptography, and DarkFi

## Installation (Optional)

This will install the binaries on your system (`/usr/local` by
default). The configuration files for the binaries are bundled with the
binaries and contain sane defaults. You'll have to run each daemon once
in order for them to spawn a config file, which you can then review.

```shell
# make install
```

### Examples and usage

See the [DarkFi book](https://dark.fi/book/)

## Go Dark

Let's liberate people from the claws of big tech and create the
democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
