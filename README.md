# Pythnet

Pythnet is an application-specific blockchain operated by Pyth's data providers. This blockchain is a computation
substrate to securely combine the data provider's prices into a single aggregate price for each Pyth price feed. Pythnet
forms the core of Pyth's off-chain price feeds that serve all blockchains.

Pythnet is powered by Solana technology: it is based on the same validator software, but is a Pyth specific chain that
is independent of Solana's mainnet. The Pyth Data Association enables each data provider to operate one validator by
delegating them the necessary stake. Once governance is live, it will take over management of validators from the Pyth
Data Association.

# Development

The Pythnet codebase is a fork of Solana's mainnet codebase. The most recent version of the codebase can be found at
branch `pyth-v1.14.17` which includes Pythnet specific changes on top of Solana's mainnet codebase at v1.14.17
(versioned with one more digit at `1.14.17X`).
