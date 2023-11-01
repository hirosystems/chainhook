---
title: Run Chainhook as a Service Using Bitcoind
---

You can run Chainhook as a service to evaluate your `if_this / then_that` predicates against the Bitcoin blockchain, delivering data—either file appendations or HTTP POST requests to a server you designate—for your application's use case. You can also dynamically register new predicates as the service is running by enabling the predicates registration API.

## Prerequisites

### Setting up a Bitcoin Node

The Bitcoin Core daemon (bitcoind) is a program that implements the Bitcoin protocol for remote procedure call (RPC) use. Chainhook can be set up to interact with the Bitcoin chainstate through bitcoind's ZeroMQ interface, its embedded networking library, passing raw blockchain data to be evaluated for relevant events.

This guide is written to work with the latest Bitcoin Core software containing bitcoind, [Bitcoin Core 25.0](https://bitcoincore.org/bin/bitcoin-core-25.0/). 

> **_NOTE:_**
>
> While bitcoind can and will start syncing a Bitcoin node, customizing this node to your use cases beyond supporting a Chainhook is out of scope for this guide. See the Bitcoin wiki for ["Running Bitcoin"](https://en.bitcoin.it/wiki/Running_Bitcoin) or bitcoin.org's [Running A Full Node guide](https://bitcoin.org/en/full-node).

- Make note of the path of your `bitcoind` executable (located within the `bin` directory of the `bitcoin-25.0` folder you downloaded above appropriate to your operating system).
- Navigate to your project folder where your Chainhook node will reside, create a new file, and rename it to `bitcoin.conf`. Copy the configuration below to this `bitcoin.conf` file. 
- Find and copy your Bitcoin data directory and paste to the `datadir` field in the `bitcoin.conf` file below. Either copy the default path (see [list of default directories by operating system](https://en.bitcoin.it/wiki/Data_directory)) or copy the custom path you set for your Bitcoin data.
- Set a username of your choice for bitcoind and use it in the `rpcuser` configuration below (`devnet` is a default).
- Set a password of your choice for bitcoind and use it in the `rpcpassword` configuration below (`devnet` is a default).

```conf
# Bitcoin Core Configuration

datadir=/path/to/bitcoin/directory/ # Path to Bitcoin directory
server=1
rpcuser=devnet
rpcpassword=devnet
rpcport=8332
rpcallowip=0.0.0.0/0
rpcallowip=::/0
txindex=1
listen=1
discover=0
dns=0
dnsseed=0
listenonion=0
rpcserialversion=1
disablewallet=0
fallbackfee=0.00001
rpcthreads=8
blocksonly=1
dbcache=4096

# Start zeromq
zmqpubhashblock=tcp://0.0.0.0:18543
```

> **_NOTE:_**
>
> The below command is a startup process that, if this is your first time syncing a node, might take a few hours to a few days to run. Alternatively, if the directory pointed to in the `datadir` field above contains bitcoin blockchain data, syncing will resume.

Now that you have the `bitcoin.conf` file ready with the bitcoind configurations, you can run the bitcoind node. The command takes the form `path/to/bitcoind -conf=path/to/bitcoin.conf`, for example:

```console
/Volumes/SSD/bitcoin-25.0/bin/bitcoind -conf=/Volumes/SSD/project/Chainhook/bitcoin.conf
```

Once the above command runs, you will see `zmq_url` entries in the console's stdout, displaying ZeroMQ logs of your bitcoin node.

### Configure Chainhook

In this section, you will configure Chainhook to match the network configurations with the bitcoin config file. First, [install the latest version of Chainhook](../getting-started.md#install-chainhook-from-source).

Next, you will generate a `Chainhook.toml` file to connect Chainhook with your bitcoind node. Navigate to the directory where you want to generate the `Chainhook.toml` file and use the following command in your terminal:

```console
chainhook config generate --mainnet
```

Several network parameters in the generated `Chainhook.toml` configuration file need to match those in the `bitcoin.conf` file created earlier in the [Setting up a Bitcoin Node](#setting-up-a-bitcoin-node) section. Please update the following parameters accordingly:

1. Update `bitcoind_rpc_username` with the username set for `rpcuser` in `bitcoin.conf`.
2. Update `bitcoind_rpc_password` with the password set for `rpcpassword` in `bitcoin.conf`.
3. Update `bitcoind_rpc_url` with the same host and port used for `rpcport` in `bitcoin.conf`.

Additionally, if you want to receive events from the configured Bitcoin node, substitute `stacks_node_rpc_url` with `bitcoind_zmq_url`, as follows:

```toml
[storage]
working_dir = "cache"

# The Http Api allows you to register / deregister
# predicates dynamically.
# This is disabled by default.

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
# stacks_node_rpc_url = "http://localhost:20443"
# but zmq can be used instead:
bitcoind_zmq_url = "tcp://0.0.0.0:18543"

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

Here is a table of the relevant parameters this guide changes in our configuration files.

| bitcoin.conf    | Chainhook.toml        |
| --------------- | --------------------- |
| rpcuser         | bitcoind_rpc_username |
| rpcpassword     | bitcoind_rpc_password |
| rpcport         | bitcoind_rpc_url      |
| zmqpubhashblock | bitcoind_zmq_url      |

## Scan blockchain based on predicates

Now that your bitcoind and Chainhook configurations are complete, you can define the Chainhook [predicates](../overview.md#if-this-predicate-design) you would like to scan against bitcoin blocks. These predicates are where you specify the kind of blockchain events that trigger Chainhook to deliver a result (either a file appendation or an HTTP POST request). This section helps you with an example JSON file to scan a range of blocks in the blockchain to trigger results. To understand the supported predicates for Bitcoin, refer to [how to use chainhooks with bitcoin](how-to-use-chainhooks-with-bitcoin.md).

The following is an example to walk you through an `if_this / then_that` predicate design that appends event payloads to the configured file destination.

### Example 1 - `file_append`

To generate a sample JSON file with predicates, execute the following command in your terminal:

```console
chainhook predicates new stacking-pool.json --bitcoin
```

Replace the contents of the `stacking-pool.json` file with the following:

```json
{
  "chain": "bitcoin",
  "uuid": "1",
  "name": "Stacking Pool",
  "version": 1,
  "networks": {
    "mainnet": {
      "start_block": 801500,
      "end_block": 802000,
      "if_this": {
        "scope": "outputs",
        "p2wpkh": {
          "equals": "bc1qs0kkdpsrzh3ngqgth7mkavlwlzr7lms2zv3wxe"
        }
      },
      "then_that": {
        "file_append": {
          "path": "bitcoin-transactions.txt"
        }
      }
    }
  }
}
```

This example demonstrates scanning a portion of the Bitcoin blockchain to capture specific outputs from a Bitcoin address associated with a Stacking pool, [Friedgar Pool](https://pool.friedger.de/).

> **_NOTE:_**
>
> You can get blockchain height and current block by referring to https://explorer.hiro.so/blocks?chain=mainnet

Now, use the following command to scan the blocks based on the predicates defined in the `stacking-pool.json` file.

```console
chainhook predicates scan stacking-pool.json --config-path=./Chainhook.toml
```

The output of the above command will be a text file `bitcoin-transactions.txt` generated based on the predicate definition.

### Example 2 - `http_post`

Let's generate another sample predicate, this time we are going to send the payload to an API endpoint:

```console
chainhook predicates new stacking-pool-api.json --bitcoin
```

Replace the contents of the `stacking-pool-api.json` file with the following:

```json
{
  "chain": "bitcoin",
  "uuid": "2",
  "name": "Stacking Pool (API)",
  "version": 1,
  "networks": {
    "mainnet": {
      "start_block": 801500,
      "if_this": {
        "scope": "outputs",
        "p2wpkh": {
          "equals": "bc1qs0kkdpsrzh3ngqgth7mkavlwlzr7lms2zv3wxe"
        }
      },
      "then_that": {
        "http_post": {
          "url": "http://localhost:3000/events",
          "authorization_header": "12345"
        }
      }
    }
  }
}
```

> **_NOTE:_**
>
> The `start_block` is a required field when using the `http_post` `then_that` predicate.

Once you are finished setting up your endpoint, use the following command to scan the blocks based on the predicates defined in the `stacking-pool-api.json` file.

```console
chainhook predicates scan stacking-pool-api.json --config-path=./Chainhook.toml
```

The above command posts events to the URL, http://localhost:3000/events mentioned in the JSON file.

## Initiate Chainhook Service

In the examples above, our Chainhook scanned historical blockchain data against predicates and delivered results. In this next section, let's learn how to set up a Chainhook that acts as an ongoing observer and event-streaming service.

We can start a Chainhook service with an existing predicate. We can also dynamically register new predicates by making an API call to our Chainhook. In both of these instances, our predicates will be delivering their results to a server set up to receive results. 

- Initiate the chainhook service by passing the predicate path to the command as shown below:

```console
chainhook service start --predicate-path=stacking-pool-api.json --config-path=Chainhook.toml
```

The above command registers the predicate based on the predicate definition in the `stacking-pool-api.json` file.

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

To dynamically register a new predicate, send a POST request to the running predicate registration server at `localhost:20456/v1/chainhooks`. Include the new predicate in JSON format within the request body. Use the following `curl` command as an example:

```console
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "chain": "bitcoin",
    "uuid": "3",
    "name": "Ordinals",
    "version": 1,
    "networks": {
      "mainnet": {
        "start_block": 777534,
        "if_this": {
          "scope": "ordinals_protocol",
          "operation": "inscription_feed"
        },
        "then_that": {
          "http_post": {
            "url": "http://localhost:3000/events",
            "authorization_header": "12345"
          }
        }
      }
    }
  }' \
  http://localhost:20456/v1/chainhooks
```

The sample response should look like this:

```jsonc
{
  "chainhook": {
    "predicate": {
      "operation": "inscription_feed",
      "scope": "ordinals_protocol"
    },
    "uuid": "1"
  },
  "apply": [
    {
      "block_identifier": {
        "hash": "0x00000000000000000003e3e2ffd3baaff2cddda7d12e84ed0ffe6f7778e988d4",
        "index": 777534
      },
      "metadata": {},
      "parent_block_identifier": {
        "hash": "0x0000000000000000000463a1034c59e6dc94c7e52855582af11882743b86e2a7",
        "index": 777533
      },
      "timestamp": 1676923039,
      "transactions": [
        {
          "transaction_identifier": {
            "hash": "0xca20efe5e4d71c16cd9b8dfe4d969efdd225ef0a26136a6a4409cb3afb2e013e"
          },
          "metadata": {
            "ordinal_operations": [
              {
                "inscription_revealed": {
                  "content_bytes": "<INSCRIPTION_BYTES>",
                  "content_length": 12293,
                  "content_type": "image/jpeg",
                  "inscriber_address": "bc1punnjva5ayg84kf5tmvx265uwvp8py3ux24skz43aycj5rzdgzjfq0jxsuc",
                  "inscription_fee": 64520,
                  "inscription_id": "ca20efe5e4d71c16cd9b8dfe4d969efdd225ef0a26136a6a4409cb3afb2e013ei0",
                  "inscription_number": 0,
                  "inscription_output_value": 10000,
                  "ordinal_block_height": 543164,
                  "ordinal_number": 1728956147664701,
                  "ordinal_offset": 1147664701,
                  "satpoint_post_inscription": "ca20efe5e4d71c16cd9b8dfe4d969efdd225ef0a26136a6a4409cb3afb2e013e:0:0",
                  "transfers_pre_inscription": 0
                }
              }
            ],
            "proof": null
          },
          "operations": []
          // Other transactions
        }
      ]
    }
  ],
  "rollback": []
}
```

Understand the output of the above JSON file with the following details.

- The `apply` payload includes the block header and the transactions that triggered the predicate.
- The `rollback` payload includes the block header and the transactions that triggered the predicate for a past block that is no longer part of the canonical chain and must be reverted. (Note: This is a chief component of Chainhook's reorg aware functionality, maintaining rollback data for blocks near the chaintip.)

> **_TIP:_**
>
> You can also run chainhook service by passing multiple predicates.
> Example: `chainhook service start --predicate-path=predicate_1.json --predicate-path=predicate_2.json --config-path=Chainhook.toml`

## References

- To learn more about Ordinals, refer to [Introducing Ordinals Explorer and Ordinals API](https://www.hiro.so/blog/introducing-the-ordinals-explorer-and-ordinals-api).
- The [OpenAPI specification for chainhook](https://raw.githubusercontent.com/hirosystems/chainhook/develop/docs/chainhook-openapi.json) is available to understand the scope of chainhook.
