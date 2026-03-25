# Part 3: Creating dchat-cli

This section covers creating a simple CLI client for interacting with `dchatd`.

## Prerequisites

Before following this section, you should have:

* Deployed a local P2P network with `dchatd` running
* Completed [Part 2: Creating dchatd](../creating-dchatd/part-2.md)

## Overview

The dchat CLI is a Python client that communicates with `dchatd` using
JSON-RPC over TCP. It provides simple commands to send messages, receive
messages, and interact with the P2P network.

## Topics Covered

* [Using dchat](using-dchat.md): How to run and use the dchat CLI
* [Python UI](ui.md): Implementation details of the CLI client

## Quick Start

1. Ensure `dchatd` is running (see [deployment instructions](../deployment/getting-started.md))
2. Send a message:
   ```shell
   python example/dchat/dchat-cli/main.py send "Hello, world!"
   ```
3. Receive messages:
   ```shell
   python example/dchat/dchat-cli/main.py recv
   ```
