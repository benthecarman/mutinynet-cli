---
name: mutinynet-cli
description: Interact with the Mutinynet Bitcoin testnet faucet. Get testnet bitcoin on-chain, pay lightning invoices, open lightning channels, and generate bolt11 invoices. Use when the user needs signet/testnet bitcoin, wants to test lightning payments, or mentions Mutinynet.
compatibility: Requires cargo (Rust toolchain) for installation and network access to faucet.mutinynet.com
metadata:
  author: benthecarman
  version: "0.1.1"
  repository: https://github.com/benthecarman/mutinynet-cli
license: MIT
allowed-tools: Bash(mutinynet-cli:*)
---

# Mutinynet Faucet CLI

A CLI tool for interacting with the Mutinynet Bitcoin testnet faucet.

## Install

Download a prebuilt binary from [GitHub Releases](https://github.com/benthecarman/mutinynet-cli/releases/latest), or install from source:

```bash
cargo install mutinynet-cli
```

## Commands

### Login

Authenticate with GitHub via device flow. Required before using commands that need a token.

```bash
mutinynet-cli login
```

### Send on-chain bitcoin

Send testnet bitcoin to an address. Default amount is 10,000 sats. Accepts Bitcoin addresses or BIP21 URIs.

```bash
mutinynet-cli onchain <address> [sats]
```

Examples:
- `mutinynet-cli onchain tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx`
- `mutinynet-cli onchain tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx 50000`

### Pay a lightning invoice

Pay a lightning invoice. Accepts bolt11 invoices, LNURL, lightning addresses, and nostr npubs.

```bash
mutinynet-cli lightning <invoice>
```

Examples:
- `mutinynet-cli lightning lnbc1...`
- `mutinynet-cli lightning user@walletofsatoshi.com`

### Open a lightning channel

Open a lightning channel from the faucet node to your node.

```bash
mutinynet-cli channel <pubkey> <capacity> [--push-amount <sats>] [--host <host:port>]
```

Examples:
- `mutinynet-cli channel 02abc...def 100000`
- `mutinynet-cli channel 02abc...def 100000 --push-amount 50000 --host localhost:9735`

### Generate a bolt11 invoice

Generate a bolt11 invoice from the faucet node. Omit amount for a zero-amount invoice.

```bash
mutinynet-cli bolt11 [amount]
```

Examples:
- `mutinynet-cli bolt11 5000`
- `mutinynet-cli bolt11`

## Configuration

| Option    | Environment Variable     | Default                        |
|-----------|--------------------------|--------------------------------|
| `--url`   | `MUTINYNET_FAUCET_URL`   | `https://faucet.mutinynet.com` |
| `--token` | `MUTINYNET_FAUCET_TOKEN` | Loaded from `~/.mutinynet/token` |

## Common workflows

1. **First time setup**: Run `mutinynet-cli login` to authenticate with GitHub.
2. **Fund a wallet**: Use `mutinynet-cli onchain <address>` to receive testnet sats.
3. **Test lightning**: Use `mutinynet-cli lightning <invoice>` to pay or `mutinynet-cli bolt11 <amount>` to receive.
