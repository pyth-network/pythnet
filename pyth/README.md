# Pyth Extensions

This module contains extensions to the Solana repository which contain
definitions for cryptographic accumulators and helpers for implementing
the bank changes required to emit them.

## What are cryptographic accumulators?

Cryptographic accumulators are data structures that allow users to compress
multiple elements belonging to the same set into a succint representation that
can still provide membership proofs. Merkle trees being an intuitive example of
this abstraction. Through cryptographic accumulators, Pyth enables users to
securely batch and send price information to other blockchains in a cheap and
efficient manner.

## Getting Started

To get started testing these functionalities, you will need to build a PythNet
validator. This can be done by running `cargo build` in the root of this
repository. 

You can run `make help` to find a set of useful commands you may find helpful
for testing.

### Quick Setup Instructions

1. Clone the PythNet repository root, and `cd` the `pyth/` directory.
2. Run `cargo build` to build the PythNet validator.
3. Run `make clone` to clone PythNet accounts locally.
4. Run `make validator` to run PythNet loaded with the accounts.

You can use traditional Solana tooling (i.e `solana -ul account <address>`) to
interact with the running test validator.
