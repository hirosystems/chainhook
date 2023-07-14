---
title: Use Chainhook with Stacks
---

This guide helps you define predicates to use Chainhook with Stacks. The predicates are specified based on `if-this`, and `then-that` constructs.

## `if_this` Specifications

The current `stacks` predicates support the following `if_this` constructs:

Get any transaction matching a given transaction ID `txid` mandatory argument admits:

- 32 bytes hex encoded type

```json

{
    "if_this": {
        "scope": "txid",
        "equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"
    }
}
```

Get any stacks block matching constraints:

`block_height` can be used to check for specific blocks based on the height of the block.

- `block_height` mandatory argument admits:
  - `equals`, `higher_than`, `lower_than`, `between`: integer type.

```json
{
    "if_this": {
        "scope": "block_height",
        "higher_than": 10000
    }
}
```

Get any transaction related to a given fungible token asset identifier:

- `asset-identifier` mandatory argument admits:
  - string type, fully qualifying the asset identifier to observe. Example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.cbtc-sip10::cbtc`
- `actions` mandatory argument admits:
  - array of string types constrained to `mint`, `transfer`, and `burn` values. Example: ["mint", "burn"]

```json
{
    "if_this": {
        "scope": "ft_event",
        "asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.cbtc-token::cbtc",
        "actions": ["burn"]
    },
}
```

Get any transaction related to a given non-fungible token asset identifier:

- `asset-identifier` mandatory argument admits:
  - string type, fully qualifying the asset identifier to observe. Example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys`
- `actions` mandatory argument admits:
  - array of string type constrained to `mint`, `transfer` and `burn` values. Example: ["mint", "burn"]

```json
{
    "if_this": {
        "scope": "nft_event",
        "asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys",
        "actions": ["mint", "transfer", "burn"]
    },
}
```

Get any transaction moving STX tokens:

- `actions` mandatory argument admits:
  - array of string type constrained to `mint`, `transfer` , `burn` and `lock` values. Example: ["mint", "lock"]

```json
{
    "if_this": {
        "scope": "stx_event",
        "actions": ["transfer", "lock"]
    },
}
```

Get any transaction emitting given print events predicate

- `contract-identifier` mandatory argument admits:
  - string type, fully qualifying the contract to observe. Example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09` `contains` mandatory argument admits:
  - string type, used for matching event

```json
{
    "if_this": {
        "scope": "print_event",
        "contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09",
        "contains": "vault"
    },
}
```

Get any transaction calling a specific method for a given contract **directly**.

> [!Warning]
> If the observed method is being called by another contract, this predicate won't detect it.

- `contract-identifier` mandatory argument admits:
  - string type, fully qualifying the contract to observe. Example: `SP000000000000000000002Q6VF78.pox` `method` mandatory argument admits: - string type, used for specifying the method to observe. Example: `stack-stx`.

```json
{
    "if_this": {
        "scope": "contract_call",
        "contract_identifier": "SP000000000000000000002Q6VF78.pox",
        "method": "stack-stx"
    },
}
```

Get any transaction, including a contract deployment:

- `deployer` mandatory argument admits:
  - string "*" - string encoding a valid STX address. Example: "ST2CY5V39NHDPWSXMW9QDT3HC3GD6Q6XX4CFRK9AG"

```json
{
    "if_this": {
        "scope": "contract_deployment",
        "deployer": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM"
    },
}
```

Get any transaction, including a contract deployment implementing a given trait
// coming soon

- `implement-trait` mandatory argument admits:

  - string type, fully qualifying the trait's shape to observe. Example: `ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.sip09-protocol`

```json
{
    "if_this": {
        "scope": "contract_deployment",
        "implement_trait": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.sip09-protocol"
    },
}
```

## `then_that` Specifications

HTTP Post block/transaction payload to a given endpoint.

- `http_post` construct admits:
  - url (string type). Example: http://localhost:3000/api/v1/wrapBtc 
  - authorization_header (string type). Secret to add to the request `authorization` header when posting payloads

```json
{
    "then_that": {
        "http_post": {
            "url": "http://localhost:3000/api/v1/wrapBtc",
            "authorization_header": "Bearer cn389ncoiwuencr"
        }
    }
}
```

Append events to a file through the filesystem. Convenient for local tests:

- `file_append` construct admits:
  - path (string type). Path to file on disk.
  
```json
{
    "then_that": {
        "file_append": {
            "path": "/tmp/events.json",
        }
    }
}
```

## Additional Configurations available

Following additional configurations can be used to improve the performance of chainhook by preventing a full scan of the blockchain:

- Ignore any block before the given block:
`"start_block": 101`

- Ignore any block after the given block:
`"end_block": 201`

- Stop evaluating chainhook after a given number of occurrences found:
`"expire_after_occurrence": 1`

- Include decoded clarity values in the payload
`"decode_clarity_values": true`

## Example predicate definition to print events

Retrieve and HTTP Post to `http://localhost:3000/api/v1/wrapBtc`  the first five transactions interacting with ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09, emitting print events containing the word 'vault'.

```json
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

## Example predicate definition with multiple networks

A specification file can also include different networks. In this case, the chainhook will select the predicate corresponding to the network it was launched against.

```json
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
