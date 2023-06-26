---
title: Getting Started
---

# Getting Started

Chainhooks interact with Bicoin and stacks layers. You can use chainhooks as a tool in your local environment and as a service in the cloud environment. 

- Chainhooks as a tool
- Chainhooks as a service

## Chainhooks as a tool

Chainhooks can be run as a tool in your local environment by understanding the steps below:




This document walks you through the installation steps and the predicate design to understand Chainhooks.

## Install Chainhooks from source

Chainhooks can be installed from the source by following the steps below:

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


## Predicate Design

Predicates are the conditions that you can define to scan the blocks easier and faster on a block chain.

Predicates are defined in the If-this, then-that format. You'll write your condition in the `if-this` condition template and use `then-that` to output the result.

### If-this predicate design

The `if-this` predicate design can use the following attributes to define the predicates. The 'scope' paramter is mandatory to use with any of the other parameters. 

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

### Then-that predicate design 

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

For more information on predicate definitions, refer to [how to use chainhook with bitcoin](how-to-se-chainhook-with-bitcoin.md) and [how to use chainhook with Stacks](how-to-use-chainhook-with-stacks.md).
