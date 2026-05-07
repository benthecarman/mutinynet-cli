# mutinynet-cli

A CLI tool for the [Mutinynet](https://mutinynet.com) faucet.

## Install

```sh
cargo install mutinynet-cli
```

## Usage

### Login

Authenticate with GitHub via device flow:

```sh
mutinynet-cli login
```

### Request on-chain bitcoin

```sh
mutinynet-cli onchain <address> [sats]
```

Default amount is 10,000 sats. Also accepts BIP21 URIs.

### Pay a lightning invoice

```sh
mutinynet-cli lightning <bolt11>
```

Supports bolt11 invoices, LNURL, lightning addresses, and nostr npubs.

### Decode a bolt11 invoice

```sh
mutinynet-cli lightning -d <bolt11>
```

Prints decoded bolt11 invoice details as JSON without attempting payment.

### Open a channel

```sh
mutinynet-cli channel <pubkey> <capacity> [--push-amount <sats>] [--host <host:port>]
```

### Generate a bolt11 invoice

```sh
mutinynet-cli bolt11 [amount]
```

Omit amount for a zero-amount invoice.

### Check your remaining daily limit

```sh
mutinynet-cli limits
```

Prints how many sats you've used and have left in the rolling 24h window.

## Configuration

| Option | Env var | Default |
|---|---|---|
| `--url` | `MUTINYNET_FAUCET_URL` | `https://faucet.mutinynet.com` |
| `--token` | `MUTINYNET_FAUCET_TOKEN` | stored token from `login` |
