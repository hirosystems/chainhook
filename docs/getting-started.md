---
title: Getting Started
---

# Getting Started

Chainhook is a re-org aware transaction indexing engine for Stacks and Bitcoin. It can extract data from blockchains based on a predicate definition. Chainhook can be used as a development tool and a service.

- Chainhook as a development tool
- Chainhook as a service

## Chainhook as a Development Tool

Chainhook can extract data from the Bitcoin and the Stacks blockchains using predicates (sometimes called `chainhooks`). A predicate specifies a rule applied as a filtering function on every block transaction. 

Chainhook has a few convenient features designed to make developers as productive as possible by iterating quickly on their local environments.


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

If you want to start using Chainhook for extracting data from Bitcoin or Stacks, you can design your predicates using the following guides: 
- [how to use chainhook with bitcoin](how-to-use-chainhook-with-bitcoin.md) and 
- [how to use chainhook with stacks](how-to-use-chainhook-with-stacks.md).

## Chainhooks as a service

You can run chainhook as a service in your cloud environment by passing the predicates dynamically.

You can also define the predicates dynamically using two options.

- Use the *predicate json file path* while starting the chainhook service.
- Use the *start-http-api* option to instantiate a REST API allowing developers to list, add, and remove predicates at runtime.

