---
title: Run Chainhook as a Service Using Stacks
---

You can run Chainhook as a service to evaluate Stacks blocks against your predicates. You can also dynamically register new predicates by enabling predicates registration API.

Start with the prerequisite section and configure your files to start the chainhook service.

## Prerequisite

### Configure Your Stacks Node

- Configure your Stacks node using the [Stacks node configuration](https://docs.stacks.co/docs/nodes-and-miners/stacks-node-configuration) documentation.
- We Recommend using the latest version of Stacks. You can check the latest version by following [this](https://github.com/stacks-network/stacks-blockchain/releases) link.
- Set up your Bitcoin node by following [this](how-to-run-chainhook-as-a-service-using-bitcoind.md#setting-up-a-bitcoin-node) article, then get the `rpcuser`, `rpcpassword`, and `rpc_port` values defined in the `bitcoin.conf` file.

A `Stacks.toml` file is generated when configuring your Stacks node. Below is the sample `Stacks.toml` file.

```toml
[node]
working_dir = "/stacks-blockchain"
rpc_bind = "0.0.0.0:20443"          # Make a note of this port to use in the `Chainhook.toml`
p2p_bind = "0.0.0.0:20444"
bootstrap_node = "02da7a464ac770ae8337a343670778b93410f2f3fef6bea98dd1c3e9224459d36b@seed-0.mainnet.stacks.co:20444,02afeae522aab5f8c99a00ddf75fbcb4a641e052dd48836408d9cf437344b63516@seed-1.mainnet.stacks.co:20444,03652212ea76be0ed4cd83a25c06e57819993029a7b9999f7d63c36340b34a4e62@seed-2.mainnet.stacks.co:20444"

[burnchain]
chain = "bitcoin"
mode = "mainnet"
peer_host = "localhost"
username = "bitcoind_username"       # Must match the rpcuser in the bitcoin.conf
password = "bitcoind_password"       # Must match the rpcpassword in the bitcoin.conf
rpc_port = 8332                      # Must match the rpcport in the bitcoin.conf
peer_port = 8333

[[events_observer]]
endpoint = "localhost:20455"
retry_count = 255
events_keys = ["*"]

```

> **_NOTE:_**
>
> Ensure that the `username`, `password`, and `rpc_port` values in the `Stacks.toml` file match the values in the `bitcoin.conf` file. Also, note the `rpc_bind` port to use in the `Chainhook.toml` configuration in the next section of this article.

### Configure Chainhook

In this section, you will configure a chainhook to communicate with the network. Run the following command in your terminal and generate the `Chainhook.toml` file.

```console
chainhook config generate --mainnet
```

Several network parameters in the generated `Chainhook.toml` configuration file need to match those in the `bitcoin.conf` file created earlier in the [Setting up a Bitcoin Node](#setting-up-a-bitcoin-node) section. Please update the following parameters accordingly:

1. Update `bitcoind_rpc_username` with the username set for `rpcuser` in `bitcoin.conf`.
2. Update `bitcoind_rpc_password` with the password set for `rpcpassword` in `bitcoin.conf`.
3. Update `bitcoind_rpc_url` with the same host and port used for `rpcport` in `bitcoin.conf`.
4. Ensure `stacks_node_rpc_url` matches the `rpc_bind` in the `Stacks.toml`.

The following `Chainhook.toml` file is generated:

```toml
[storage]
working_dir = "cache"

# The Http Api allows you to register / deregister
# dynamically predicates.
# Disable by default.
#
# [http_api]
# http_port = 20456
# database_uri = "redis://localhost:6379/"

[network]
mode = "mainnet"
bitcoind_rpc_url = "http://localhost:8332"
bitcoind_rpc_username = "devnet"
bitcoind_rpc_password = "devnet"
# Bitcoin block events can be received by Chainhook
# either through a Bitcoin node's ZeroMQ interface,
# or through the Stacks node. The Stacks node is
# used by default:
stacks_node_rpc_url = "http://localhost:20443"
stacks_events_ingestion_port = 20455
# but zmq can be used instead:
# bitcoind_zmq_url = "tcp://0.0.0.0:18543"

[limits]
max_number_of_bitcoin_predicates = 100
max_number_of_concurrent_bitcoin_scans = 100
max_number_of_stacks_predicates = 10
max_number_of_concurrent_stacks_scans = 10
max_number_of_processing_threads = 16
max_number_of_networking_threads = 16
max_caching_memory_size_mb = 32000

[[event_source]]
tsv_file_url = "https://archive.hiro.so/mainnet/stacks-blockchain-api/mainnet-stacks-blockchain-api-latest"
```

Ensure the following configurations are matched to allow chainhook to communicate with both Stacks and Bitcoin.

| bitcoin.conf    | Stacks.toml | Chainhook.toml               |
| --------------- | ----------- | ---------------------------- |
| rpcuser         | username    | bitcoind_rpc_username        |
| rpcpassword     | password    | bitcoind_rpc_password        |
| rpcport         | rpc_port    | bitcoind_rpc_url             |
| zmqpubhashblock |             | bitcoind_zmq_url             |
|                 | rpc_bind    | stacks_node_rpc_url          |
|                 | endpoint    | stacks_events_ingestion_port |

> **_NOTE:_**
>
> The `bitcoind_zmq_url` is optional when running chainhook as a service using Stacks because Stacks will pull the blocks from Stacks and the Bitcoin chain.

## Scan the blockchain based on predicates

Now that the Stacks and Chainhook configurations are done, you can scan your blocks by defining your [predicates](../overview.md#if-this-predicate-design). This section helps you with sample JSON files to scan blockchain blocks and render the results. To understand the supported predicates for Stacks, refer to [how to use chainhook with stacks](how-to-use-chainhooks-with-stacks.md).

The following are the two examples to walk you through `file_append` and `http_post` `then-that` predicate designs.

Example 1 uses a `print-event.json` file to scan the predicates and render results using `file_append`.
Example 2 uses `print-event-post.json` to scan the predicates and render results using `http_post`.

You can choose between the following examples to scan the predicates.

### Example 1 - `file_append`

Run the following command to generate a sample JSON file with predicates in your terminal.

```console
chainhook predicates new print-event.json --stacks
```

A JSON file `print-event.json` is generated.

```json
{
  "chain": "stacks",
  "uuid": "6ad27176-2b83-4381-b51c-50baede11e3f",
  "name": "Hello world",
  "version": 1,
  "networks": {
    "testnet": {
      "start_block": 34239,
      "end_block": 50000,
      "if_this": {
        "scope": "print_event",
        "contract_identifier": "ST1SVA0SST0EDT4MFYGWGP6GNSXMMQJDVP1G8QTTC.arkadiko-freddie-v1-1",
        "contains": "vault"
      },
      "then_that": {
        "file_append": {
          "path": "arkadiko.txt"
        }
      }
    },
    "mainnet": {
      "start_block": 34239,
      "end_block": 50000,
      "if_this": {
        "scope": "print_event",
        "contract_identifier": "SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1",
        "contains": "vault"
      },
      "then_that": {
        "file_append": {
          "path": "arkadiko.txt"
        }
      }
    }
  }
}
```

> **_NOTE:_**
>
> You can get blockchain height and current block in the [Explorer](https://explorer.hiro.so/blocks?chain=mainnet).

Now, use the following command to scan the blocks based on the predicates defined in the `mainnet` network block of your `print-event.json` file:

```console
chainhook predicates scan print-event.json --mainnet
```

The output of the above command will be a text file `arkadiko.txt` generated based on the predicate definition. It should look something like this:

```
{"apply":[{"block_identifier":{"hash":"0xf048102fee15dda049e6781c8e9aec1b39b1b9dc68d06fd9b84dced1b80ddd62","index":34307},"metadata":{"bitcoin_anchor_block_identifier":{"hash":"0x000000000000000000098e9ebc30e7c8e32b30ffecbd7dc5c715b5f07e1de25c","index":705648},"confirm_microblock_identifier":{"hash":"0xa65642590e98f54183a0be747a1c01e41d3ba211f6599eff2574d78ed2578468","index":2},"pox_cycle_index":18,"pox_cycle_length":2100,"pox_cycle_position":1797,"stacks_block_hash":"0x77a1aed86e895cb4b7b969986aa6a28eb2465e7227f351dd4e23d28448b222e9"},"parent_block_identifier":{"hash":"0x3117663ee5c5690d76e3f6c97597cbcc95085e7cecb0791d3edc4f95a4ce6f23","index":34306},"timestamp":1634625398,"transactions":[{"metadata":{"description":"invoked: SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1::collateralize-and-mint(u300000000, u130000000, (tuple (auto-payoff true) (stack-pox true)), \"STX-A\", SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-stx-reserve-v1-1, SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-token, SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-collateral-types-v1-1, SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-oracle-v1-1)","execution_cost":{"read_count":155,"read_length":318312,"runtime":349859000,"write_count":10,"write_length":3621},"fee":188800,"kind":{"data":{"args":["u300000000","u130000000","(tuple (auto-payoff true) (stack-pox true))","\"STX-A\"","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-stx-reserve-v1-1","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-token","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-collateral-types-v1-1","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-oracle-v1-1"],"contract_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1","method":"collateralize-and-mint"},"type":"ContractCall"},"nonce":15,"position":{"index":16},"proof":null,"raw_tx":"0x000000000104003936cedf1ddb6bc1aa6f243772cab048c586a18b000000000000000f000000000002e1800001b563c7e917648668a796d972ef9352b76035102c6159c2061bc9e9a0d161098e1ebd17d6c0754e0308d2b53ef1035e5dcfba37f0cc6f16e69f842ad7f6b691980302000000010002163936cedf1ddb6bc1aa6f243772cab048c586a18b010000000011e1a3000216982f3ec112a5f5928a5c96a914bd733793b896a51561726b6164696b6f2d667265646469652d76312d3116636f6c6c61746572616c697a652d616e642d6d696e74000000080100000000000000000000000011e1a3000100000000000000000000000007bfa4800c000000020b6175746f2d7061796f66660309737461636b2d706f78030d000000055354582d410616982f3ec112a5f5928a5c96a914bd733793b896a51961726b6164696b6f2d7374782d726573657276652d76312d310616982f3ec112a5f5928a5c96a914bd733793b896a50e61726b6164696b6f2d746f6b656e0616982f3ec112a5f5928a5c96a914bd733793b896a51e61726b6164696b6f2d636f6c6c61746572616c2d74797065732d76312d310616982f3ec112a5f5928a5c96a914bd733793b896a51461726b6164696b6f2d6f7261636c652d76312d31","receipt":{"contract_calls_stack":[],"events":[{"data":{"amount":"130000000","asset_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.usda-token::usda","recipient":"SPWKDKPZ3QDPQGDADWJ3EWPAP14CB1N1HDQ897W5"},"type":"FTMintEvent"},{"data":{"contract_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1","raw_value":"0x0c0000000306616374696f6e0d000000076372656174656404646174610c000000110d61756374696f6e2d656e646564040b6175746f2d7061796f6666030a636f6c6c61746572616c0100000000000000000000000011e1a30010636f6c6c61746572616c2d746f6b656e0d000000035354580f636f6c6c61746572616c2d747970650d000000055354582d4117637265617465642d61742d626c6f636b2d686569676874010000000000000000000000000000860304646562740100000000000000000000000007bfa48002696401000000000000000000000000000000010d69732d6c69717569646174656404136c6566746f7665722d636f6c6c61746572616c0100000000000000000000000000000000056f776e657205163936cedf1ddb6bc1aa6f243772cab048c586a18b107265766f6b65642d737461636b696e67041573746162696c6974792d6665652d6163637275656401000000000000000000000000000000001a73746162696c6974792d6665652d6c6173742d6163637275656401000000000000000000000000000086030e737461636b65642d746f6b656e730100000000000000000000000011e1a3000c737461636b65722d6e616d650d00000007737461636b657217757064617465642d61742d626c6f636b2d686569676874010000000000000000000000000000860304747970650d000000057661756c74","topic":"print"},"type":"SmartContractEvent"},{"data":{"amount":"300000000","recipient":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-stx-reserve-v1-1","sender":"SPWKDKPZ3QDPQGDADWJ3EWPAP14CB1N1HDQ897W5"},"type":"STXTransferEvent"},{"data":{"contract_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-dao","raw_value":"0x0c0000000306616374696f6e0d000000066d696e74656404646174610c0000000206616d6f756e740100000000000000000000000007bfa48009726563697069656e7405163936cedf1ddb6bc1aa6f243772cab048c586a18b04747970650d00000005746f6b656e","topic":"print"},"type":"SmartContractEvent"},{"data":{"contract_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-stx-reserve-v1-1","raw_value":"0x0703","topic":"print"},"type":"SmartContractEvent"}],"mutated_assets_radius":["SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.usda-token::usda"],"mutated_contracts_radius":["SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-dao","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-stx-reserve-v1-1","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1","SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.usda-token"]},"result":"(ok u130000000)","sender":"SPWKDKPZ3QDPQGDADWJ3EWPAP14CB1N1HDQ897W5","success":true},"operations":[{"account":{"address":"SPWKDKPZ3QDPQGDADWJ3EWPAP14CB1N1HDQ897W5"},"amount":{"currency":{"decimals":6,"metadata":{"asset_class_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.usda-token::usda","asset_identifier":null,"standard":"SIP10"},"symbol":"TOKEN"},"value":130000000},"operation_identifier":{"index":0},"status":"SUCCESS","type":"CREDIT"},{"account":{"address":"SPWKDKPZ3QDPQGDADWJ3EWPAP14CB1N1HDQ897W5"},"amount":{"currency":{"decimals":6,"symbol":"STX"},"value":300000000},"operation_identifier":{"index":1},"related_operations":[{"index":2}],"status":"SUCCESS","type":"DEBIT"},{"account":{"address":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-stx-reserve-v1-1"},"amount":{"currency":{"decimals":6,"symbol":"STX"},"value":300000000},"operation_identifier":{"index":2},"related_operations":[{"index":1}],"status":"SUCCESS","type":"CREDIT"}],"transaction_identifier":{"hash":"0x580d89b79f4e7cda9e2ae9f1a70a5392149a055b0b6f25968afb80c6cc09306a"}}]}],"chainhook":{"is_streaming_blocks":false,"predicate":{"contains":"vault","contract_identifier":"SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1","scope":"print_event"},"uuid":"6ad27176-2b83-4381-b51c-50baede11e3f"},"rollback":[]}
```

> **_TIP:_**
> To optimize your experience with scanning, there are a few variables you can play with:
> Use of adequate values for `start_block` and `end_block` in predicates will drastically improve performance.
> Networking: reducing the number of network hops between the chainhook and the bitcoind processes can also help.

### Example 2 - `http_post`

Run the following command to generate a sample JSON file with predicates in your terminal:

```console
chainhook predicates new print-event-post.json --stacks
```

Update the generated JSON file `print-event-post.json` with the following:

```json
{
  "chain": "stacks",
  "uuid": "e5fa09b2-ec3e-4b6a-9a4a-0ebb454f6e19",
  "name": "Hello world",
  "version": 1,
  "networks": {
    "testnet": {
      "if_this": {
        "scope": "print_event",
        "contract_identifier": "ST1SVA0SST0EDT4MFYGWGP6GNSXMMQJDVP1G8QTTC.arkadiko-freddie-v1-1",
        "contains": "vault"
      },
      "then_that": {
        "http_post": {
          "url": "http://localhost:3000/events",
          "authorization_header": "Bearer cn389ncoiwuencr"
        }
      },
      "start_block": 10200,
      "expire_after_occurrence": 5
    },
    "mainnet": {
      "if_this": {
        "scope": "print-event",
        "contract_identifier": "SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1",
        "contains": "vault"
      },
      "then_that": {
        "http_post": {
          "url": "http://localhost:3000/events",
          "authorization_header": "Bearer cn389ncoiwuencr"
        }
      },
      "start_block": 10200,
      "expire_after_occurrence": 5
    }
  }
}
```

> **_NOTE:_**
>
> The `start_block` is the required field to use the `http_post` `then-that` predicate.

Now, use the following command to scan the blocks based on the predicates defined in the `print-event-post.json` file:

```console
chainhook predicates scan print-event-post.json --mainnet
```

The above command posts events to the URL `http://localhost:3000/events` mentioned in the `Chainhook.toml` file.

## Initiate Chainhook Service

In this section, you'll learn two ways to initiate the Chainhook service as well as how to use the REST API call to post the events onto a server.

- Initiate the Chainhook service by passing the predicate path to the command as shown below:

  ```console
  chainhook service start --predicate-path=print-event.json --config-path=Chainhook.toml
  ```

  The above command registers the predicates based on the predicate definition in the `print-event.json` file.

## Dynamically Register Predicates

You can also dynamically register new predicates with your Chainhook service.

First, we need to uncomment the following lines of code in the `Chainhook.toml` file to enable the predicate registration server:

```toml
# ...

[http_api]
http_port = 20456
database_uri = "redis://localhost:6379/"

# ...
```

> **_NOTE:_**
>
> This assumes you have a local instance of [Redis](https://redis.io/docs/getting-started/) running.

Start the Chainhook service by running the following command:

```
chainhook service start --config-path=Chainhook.toml
```

To dynamically register a new predicate, send a POST request to the running predicate registration server at `localhost:20456/v1/chainhooks`. Include the new predicate in JSON format within the request body. In another terminal window, use the following `curl` command as an example:

```console
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "chain": "stacks",
    "uuid": "42",
    "name": "Arkadiko",
    "version": 1,
    "networks": {
      "mainnet": {
        "start_block": 777534,
        "if_this": {
          "scope": "print-event",
          "contract_identifier": "SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1",
          "contains": "vault"
        },
        "then_that": {
          "http_post": {
            "url": "http://localhost:3000/events",
            "authorization_header": "Bearer cn389ncoiwuencr"
          }
        }
      }
    }
  }' \
  http://localhost:20456/v1/chainhooks
```

You should see in your terminal:

```console
{"result":"42","status":200}
```

And if you hop back over to your `Chainhook` service terminal window, you will see that your predicate has been registered.

> **_TIP:_**
>
> You can also run chainhook service by passing multiple predicates.
> Example: `chainhook service start --predicate-path=predicate_1.json --predicate-path=predicate_2.json --config-path=Chainhook.toml`
