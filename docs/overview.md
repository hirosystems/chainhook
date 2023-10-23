---
title: Overview
---

# Chainhook Overview

Chainhook is a reorg-aware transaction indexing engine that helps you get reliable blockchain data, regardless of forks and reorgs. By focusing only on the data developers care about, Chainhook helps devs work with much lighter datasets and build IFTTT logic into their applications.

Chainhook can be used as a tool in your local development environment and as a service in the cloud environment.

## What problem does it solve?

Today, Bitcoin and web3 developers struggle to get reliable blockchain data to power their applications due to forks and reorgs. 

Developers who build applications and services often need to build their own index of the blockchain chainstate. For accurate data, they have to re-index each time there is a reorg of the chainstate, which happens often. Re-indexing is a massive pain point impacting developers building on Bitcoin. 

With Chainhook, developers can build consistent, reorg-proof databases that index only the information they want and trigger actions in response to on-chain events using IFTTT (if_this, then_that) logic.

## Features

1. **Faster, More Efficient Indexing:** Instead of working with a generic blockchain indexer, taking hours to process every single transaction of every single block, you can create your own index, build, iterate, and refine it in minutes. Chainhook can help you avoid massive storage management and storage scaling issues by avoiding full chain indexation. Lighter indexes lead to faster query results, which helps minimize end-user response time. This leads to a better developer experience and a better end-user experience.

2. **Re-org and Fork Aware:** Chainhook stores possible chain forks and checks each new chain event against the forks to maintain the current valid fork. All triggers, also known as **predicates**, are evaluated against the current valid fork. In the event of a reorg, Chainhook computes a list of new blocks to apply and old blocks to rollback and evaluates the registered predicates against those blocks.
  
3. **IFTTT Logic, powering your applications:** Chainhook helps developers create elegant event-based architectures using triggers, also known as **predicates**. Developers can write “if_this / then_that” **predicates** that when triggered, are packaged as events and forwarded to the configured destination. By using cloud functions as destinations, developers can also cut costs on processing by only paying for processing when a block that contains some data relevant to the developer's application is being mined.

## Chainhooks: Trigger IFTTT Logic in your Application

With Chainhook, you can trigger actions based on predicates you define. Chainhooks can be triggered by events such as:

- A certain amount of SIP-10 tokens were transferred
- A particular blockchain address received some tokens on the Stacks/Bitcoin blockchain
- A particular print event was emitted by a contract
- A particular contract was involved in a transaction
- A quantity of BTC was received at a Bitcoin address
- A POX transfer occurred on the Bitcoin chain

## Understand the Predicate Design

Predicates are conditions you can define to scan the blocks easier and faster on a blockchain.

Predicates are defined in the If-this, then-that format. You'll write your condition in the `if-this` condition template and use `then-that` to output the result.

### `if-this` Predicate Design

The `if-this` predicate design can use the following attributes to define the predicates. The 'scope' parameter is mandatory to use with any other parameters.

- scope (mandatory)
- equals
- op_return
  - ends_with
- p2pkh
- p2sh
- p2wpkh
- operation

**Example:**

```json

{
    "if_this": {
        "scope": "txid",
        "equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"
    }
}
```

### `then-that` Predicate Design

The `then-that` predicate design can use the following attributes to output the result based on the `if-this` condition defined.

- http_post
  - url
  - authorization_header
- file_append
  - path

**Example:**

```jsonc
{
    "then_that": {
        "file_append": {
            "path": "/tmp/events.json",
        }
    }
}
```

For more information on predicate definitions, refer to [how to use chainhooks with bitcoin](./how-to-guides/how-to-use-chainhooks-with-bitcoin.md) and [how to use chainhooks with Stacks](./how-to-guides/how-to-use-chainhooks-with-stacks.md).
