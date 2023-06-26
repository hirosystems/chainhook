---
title: Use Chainhook with Stacks
---

# Use chainhook with Stacks

The following guide helps you define predicates to use chainhook with Stacks.

## Guide to `if_this` / `then_that` predicate design

To get started with Stacks predicates, we can use the `chainhook` to generate a template: 

```bash
$ chainhook predicates new hello-arkadiko.json --stacks
```

*******************Explain the above command and disccuss about Chainhooks vs Chainhooks********************

## `if_this` and `then_that` specifications

*******************Are the below conditions specific to Stacks blockchain?
If the stacks blockchain adds new function, can our chainhooks need a new if-this condition?
************************

The current `stacks` predicates support the following `if_this` constructs:

Get any transaction matching a given `txid` mandatory argument admits:
- 32 bytes hex encoded type. 

Example:

```json
 
{
    "if_this": {
        "scope": "txid",
        "equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"
    }
}
```
```
// Get any stacks block matching constraints
// `block_height` mandatory argument admits:
//  - `equals`, `higher_than`, `lower_than`, `between`: integer type.
{
    "if_this": {
        "scope": "block_height",
        "higher_than": 10000
    }
}
```
```
// Get any transaction related to a given fungible token asset identifier
// `asset-identifier` mandatory argument admits:
//  - string type, fully qualifying the asset identifier to observe. example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.cbtc-sip10::cbtc`
// `actions` mandatory argument admits:
//  - array of string type constrained to `mint`, `transfer` and `burn` values. example: ["mint", "burn"]
{
    "if_this": {
        "scope": "ft_event",
        "asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.cbtc-token::cbtc",
        "actions": ["burn"]
    },
}
```
```
// Get any transaction related to a given non fungible token asset identifier
// `asset-identifier` mandatory argument admits:
//  - string type, fully qualifying the asset identifier to observe. example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys`
// `actions` mandatory argument admits:
//  - array of string type constrained to `mint`, `transfer` and `burn` values. example: ["mint", "burn"]
{
    "if_this": {
        "scope": "nft_event",
        "asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys",
        "actions": ["mint", "transfer", "burn"]
    },
}
```
```
// Get any transaction moving STX tokens
// `actions` mandatory argument admits:
//  - array of string type constrained to `mint`, `transfer` and `lock` values. example: ["mint", "lock"]
{
    "if_this": {
        "scope": "stx_event",
        "asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys",
        "actions": ["transfer", "lock"]
    },
}
```
```
// Get any transaction emitting given print events predicate
// `contract-identifier` mandatory argument admits:
//  - string type, fully qualifying the contract to observe. example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09`
// `contains` mandatory argument admits:
//  - string type, used for matching event
{
    "if_this": {
        "scope": "print_event",
        "contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09",
        "contains": "vault"
    },
}
```
```
// Get any transaction calling a specific method for a given contract **directly**.
// Warning: if the watched method is being called by another contract, this predicate won't detect it.
// `contract-identifier` mandatory argument admits:
//  - string type, fully qualifying the contract to observe. example: `SP000000000000000000002Q6VF78.pox`
// `method` mandatory argument admits:
//  - string type, used for specifying the method to observe. example: `stack-stx`
{
    "if_this": {
        "scope": "contract_call",
        "contract_identifier": "SP000000000000000000002Q6VF78.pox",
        "method": "stack-stx"
    },
}
```
```
// Get any transaction including a contract deployment
// `deployer` mandatory argument admits:
//  - string "*"
//  - string encoding a valid STX address. example: "ST2CY5V39NHDPWSXMW9QDT3HC3GD6Q6XX4CFRK9AG"
{
    "if_this": {
        "scope": "contract_deployment",
        "deployer": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM"
    },
}
```
```
// Get any transaction including a contract deployment implementing a given trait (coming soon)
// `implement-trait` mandatory argument admits:
//  - string type, fully qualifying the trait's shape to observe. example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.sip09-protocol`
{
    "if_this": {
        "scope": "contract_deployment",
        "implement_trait": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.sip09-protocol"
    },
}
```
In terms of actions available, the following then_that constructs are supported:
```
// HTTP Post block / transaction payload to a given endpoint.
// `http_post` construct admits:
//  - url (string type). Example: http://localhost:3000/api/v1/wrapBtc
//  - authorization_header (string type). Secret to add to the request `authorization` header when posting payloads
{
    "then_that": {
        "http_post": {
            "url": "http://localhost:3000/api/v1/wrapBtc",
            "authorization_header": "Bearer cn389ncoiwuencr"
        }
    }
}
```
```
// Append events to a file through filesystem. Convenient for local tests.
// `file_append` construct admits:
//  - path (string type). Path to file on disk.
{
    "then_that": {
        "file_append": {
            "path": "/tmp/events.json",
        }
    }
}
```
Additional configuration knobs available:

// Ignore any block prior to given block:
"start_block": 101

// Ignore any block after given block:
"end_block": 201

// Stop evaluating chainhook after a given number of occurrences found:
"expire_after_occurrence": 1

// Include decoded clarity values in payload
"decode_clarity_values": true
Putting all the pieces together:

// Retrieve and HTTP Post to `http://localhost:3000/api/v1/wrapBtc` 
// the 5 first transactions interacting with ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09,
// emitting print events containing the word 'vault'.

```
{
  "chain": "stacks",
  "uuid": "1",
  "name": "Lorem ipsum",
  "version": 1,
  "networks": {
    "testnet": {
        "if_this": {
            "scope": "print_event",
            "contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09",
            "contains": "vault"
        },
        "then_that": {
            "http_post": {
            "url": "http://localhost:3000/api/v1/vaults",
            "authorization_header": "Bearer cn389ncoiwuencr"
            }
        },
        "start_block": 10200,
        "expire_after_occurrence": 5,
    }
  }
}
```

```

// A specification file can also include different networks.
// In this case, the chainhook will select the predicate
// corresponding to the network it was launched against.
{
  "chain": "stacks",
  "uuid": "1",
  "name": "Lorem ipsum",
  "version": 1,
  "networks": {
    "testnet": {
        "if_this": {
            "scope": "print_event",
            "contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09",
            "contains": "vault"
        },
        "then_that": {
            "http_post": {
                "url": "http://localhost:3000/api/v1/vaults",
                "authorization_header": "Bearer cn389ncoiwuencr"
            }
        },
        "start_block": 10200,
        "expire_after_occurrence": 5,
    },
    "mainnet": {
        "if_this": {
            "scope": "print_event",
            "contract_identifier": "SP456HQKV0RJXZFY1DGX8MNSNYVE3VGZJSRT459863.monkey-sip09",
            "contains": "vault"
        },
      "then_that": {
            "http_post": {
                "url": "http://my-protocol.xyz/api/v1/vaults",
                "authorization_header": "Bearer cn389ncoiwuencr"
            }
      },
      "start_block": 90232,
      "expire_after_occurrence": 5,
    }
  }
}
```

## Guide to local Stacks testnet / mainnet predicate scanning

Developers can test their Stacks predicates without spinning up a Stacks node.
To date, the Stacks blockchain has just over two years of activity, and the `chainhook` utility can work with both `testnet` and `mainnet` chainstates in memory.  

To test a Stacks `if_this` / `then_that` predicate, the following command can be used:

```bash
$ chainhook predicates scan ./path/to/predicate.json --testnet
```

The first time this command run, a chainstate archive will be downloaded, uncompressed, and written to disk (around 3GB required for the testnet and 10GB for the mainnet).

The subsequent scans will use the cached chainstate if already present, speeding up iterations and the overall feedback loop. 
