---
title: Getting Started
---

# Getting Started

Chainhook is re-org aware transaction indexing engine for Stacks and Bitcoin. It can extract data from blockchains based on a predicate definition. Chainhook can be used as a development tool and a service.

- Chainhooks as a tool
- Chainhooks as a service

## Chainhooks as a tool

Chainhooks are often described using the term predicates, which are defined as conditions to scan your blocks on the blockchain. Chainhooks can be run as a tool in your local environment by defining, scanning and deploying your predicates.

Once you are ready to run chainhook as a tool in your local environment, you can install chainhook by following the next section.

### Install Chainhook from the source

Chainhook can be installed from the source by following the steps below:

1. Clone the [chainhook repo](https://github.com/hirosystems/chainhook/) by using the following command.
   
   ```bash
   git clone https://github.com/hirosystems/chainhook.git
   ```

2. Navigate to the root directory of the cloned repo.
   
   ```bash
   cd chainhook
   ```

3. Run cargo target to install chainhook.
   
    ```bash
    cargo chainhook-install
    ```

If you want to run chainhook as a service using Bitcoin, Stacks, you can understand the predicate design by following: 
- [how to use chainhook with bitcoin](how-to-use-chainhook-with-bitcoin.md) and 
- [how to use chainhook with stacks](how-to-use-chainhook-with-stacks.md).

## Chainhooks as a service

You can run chainhook as a service in your cloud environment by passing the predicates dynamically.

You can also define the predicates dynamically using two options.

- Use the *predicate json file path* while starting the chainhook service.
- Use the *start-http-api* option to instantiate a REST API allowing developers to list, add, and remove predicates at runtime.

