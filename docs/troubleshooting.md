---
title: Troubleshooting guide
---

# Troubleshooting Guide

This guide covers common issues you may encounter when setting up or running Chainhook, along with their solutions.

## Installation Issues

### Rust toolchain not found

If you see an error like `rustup: command not found` or `cargo: command not found`, you need to install the Rust toolchain first:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Build fails with missing dependencies

On Ubuntu/Debian, you may need to install build essentials and OpenSSL dev headers:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev
```

On macOS, ensure Xcode command line tools are installed:

```bash
xcode-select --install
```

## Configuration Issues

### Chainhook fails to connect to the Bitcoin node

- Verify that `bitcoind` is running and the RPC port is accessible.
- Check the `bitcoin_node_rpc_url` in your `Chainhook.toml` matches your node's configuration.
- Ensure the RPC username and password are correct.

### Chainhook fails to connect to the Stacks node

- Verify that `stacks-node` is running and the RPC endpoint is accessible.
- Check `stacks_node_rpc_url` in your configuration.
- If using Docker, make sure the containers are on the same network.

## Runtime Issues

### Predicate not triggering

- Double-check your predicate definition for typos in contract addresses, function names, or event types.
- Ensure the `start_block` is set to a block height before the expected event.
- Verify the `chain` field matches the network you are targeting (`mainnet` or `testnet`).

### High memory usage during scanning

When scanning large block ranges, Chainhook may consume significant memory. Consider:

- Narrowing the `start_block` and `end_block` range in your predicate.
- Running scanning operations on a machine with sufficient RAM (4 GB+ recommended).
- Using `end_block` to limit the scan to a specific range before extending it.

### Webhook endpoint not receiving payloads

- Ensure your `http_post` URL is reachable from the machine running Chainhook.
- Check firewall rules to make sure the port is open.
- Look at Chainhook logs for delivery errors or HTTP status codes.

## Getting Help

If you are still experiencing issues:

- Search for existing issues on the [Chainhook GitHub repository](https://github.com/hirosystems/chainhook/issues).
- Open a new issue with detailed logs and your predicate configuration (remove any sensitive information).
- Join the [Stacks Discord](https://discord.gg/stacks) for community support.
