---
title: FAQs
---

# FAQs

#### **Can Chainhook target both Bitcoin and Stacks?**

Chainhooks can listen and act on events from the Bitcoin and Stacks network.

#### **Can I use chainhook for cross-chain protocols?**

Yes, Chainhooks can be used for coordinating cross-chain actions. You can use chainhook on Bitcoin, ordinals, and Stacks.

#### **Can I use chainhook for chain-indexing?**

Chainhook can easily extract the information needed to build (or rebuild) databases for a front end.

#### **Can I use Chainhook with distributed nodes?**

The chainhook event observer was designed as a library written in Rust, which makes it very portable. Bindings can easily be created from other languages (Node, Ruby, Python, etc.), making this tool a very convenient and performant library, usable by anyone.

#### **How can I connect chainhook with Oracles?**

Oracles, in general, do the following:

 1. Capture relevant on-chain events
 2. Process the events via some off-chain, centralized logic
 3. Commit the resultant data on-chain

 Chainhook can be used to efficiently capture relevant on-chain events and forward them to off-chain services.

#### **How can I use Chainhook in my application?**

Chainhook can be used from the exposed RESTful API endpoints. A comprehensive OpenAPI specification explaining how to interact with the Chainhook REST API can be found [here](https://raw.githubusercontent.com/hirosystems/chainhook/develop/docs/chainhook-openapi.json).

#### **Can I run chainhook on mainnet?**

Yes, you can run chainhook on both the testnet and mainnet.

#### **How can I optimize chainhook scanning?**

Use adequate values for `start_block` and `end_block` in predicates by reducing the number of network hops between the chainhook and the `bitcoind` process.
