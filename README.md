# Chainhook

Build reliable blockchain event-driven applications with a reorg-aware transaction indexing engine.

- 🚀 **Lightning-fast custom indexes** - Skip full chain indexing. Create lightweight, targeted indexes for only the data you need
- 🔄 **Reorg and fork aware** - Automatically handles chain reorganizations, ensuring your triggers are always evaluated against the canonical chain
- ⚡ **IFTTT for blockchains** - Write "if_this, then_that" predicates that trigger actions when specific on-chain events occur

---

## Documentation

- [Chainhook Documentation](https://docs.hiro.so/stacks/chainhook)

---

## Quickstart

```bash
# Install Chainhook
brew install chainhook

# Create a new predicate
chainhook predicates new my-predicate.json --stacks

# Scan blocks with your predicate  
chainhook predicates scan ./my-predicate.json --testnet

# Or run as a service for real-time streaming
chainhook service start --predicate-path=./my-predicate.json --config-path=./config.toml

Example predicate:

```json
{
  "chain": "stacks",
  "uuid": "1",
  "name": "contract-call-hook",
  "version": 1,
  "networks": {
    "testnet": {
      "if_this": {
        "scope": "contract_call",
        "contract_identifier": "STJ81C2WPQHFB6XTG518JKPABWM639R2X37VFKJV.simple-vote-v0",
        "method": "cast-vote"
      },
      "then_that": {
        "file_append": {
          "path": "/tmp/events.json"
        }
      },
      "start_block": 21443
      // Additional configurations
    },
    "mainnet": {
      "if_this": {
        "scope": "contract_call",
        "contract_identifier": "STJ81C2WPQHFB6XTG518JKPABWM639R2X37VFKJV.simple-vote-v0",
        "method": "cast-vote"
      },
      "then_that": {
        "http_post": {
          "url": "http://my-protocol.xyz/api/v1/ordinals",
          "authorization_header": "Bearer cn389ncoiwuencr"
        }
      },
      "start_block": 142221
      // Additional configurations
    }
  }
}
```

> For detailed predicate syntax, installation options, and advanced configurations, check out our [documentation](https://docs.hiro.so/stacks/chainhook).

## Contributing

We welcome contributions! Please see our [contributing guide](.github/CONTRIBUTING.md).

## Community

Join our community and stay connected with the latest updates and discussions:

- [Join our Discord community chat](https://discord.gg/ZQR6cyZC) to engage with other users, ask questions, and participate in discussions.

- [Visit hiro.so](https://www.hiro.so/) for updates and subcribing to the mailing list.

- Follow [Hiro on X.](https://x.com/hirosystems)
