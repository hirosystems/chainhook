---
title: Run Chainhook as a service using Bitcoind
---

# Run Chainhook as a service using Bitcoind

Chainhoook node can interact with Bitcoin and Stacks blockchains. However, chainhook can get blocks from either bitcoind or Stacks. This document helps you with running chainhook as a service using Bitcoind. To run chainhook as a service using Stacks, refer to [this](run-chainhook-as-a-service-using-stacks.md).

## Prerequisites

- Install bitcoind using [this](https://bitcoin.org/en/bitcoin-core/) link.
- Install the latest version of chainhook by following [Install Chainhooks from source](../getting-started.md#install-chainhooks-from-source).

Bitcoind installation will download binaries in a zip format, `bitcoin-22.0-osx64.tar.gz`. You can extract the zip file to view the folders. Expand the `bin` folder to see executable files. The `bitcoind` executable file also exists here.

## Steps to run chainhooks as a service

To run chainhook as a service, you will follow the steps:

- Ingesting blocks through bitcoind
  - To understand predicates for bitcoind, refer to [how to use chainhook with bitcoind](how-to-use-chainhook-with-bitcoin.md)
- Test your predicates
- Deploy your predicates

## Ingesting blocks through bitcoind

Ingesting blocks happens through zeromq, an embedded networking library in the bitcoind installation package.

### Prepare the bitcoind node

- Create a config file `bitcoin.conf` on your local machine, use the configurations below and save it.
- Get the downloaded path of the Bitcoind from the [prerequisites section](#prerequisites) and use it in the `datadir` configuration below.
- Set a username of your choice for bitcoind and use it in the `rpcuser` configuration below.
- Set a password of your choice for bitcoind and use it in the `rpcpassword` configuration below.

>[!NOTE]
> Make a note of the `rpcuser`, `rpcpassword` and `rpcport` values to use later in the chainhook configuration.


```conf
datadir=<path-to-your-downloaded-bitcoind>
server=1
rpcuser="bitcoind_username"  --> You can set the username here
rpcpassword="bitcoind_password"  --> You can set the password here
rpcport=8332   --> You can set your localhost port number here
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

### Run the bitcoind node

Now that you have `bitcoin.conf` file ready with the bitcoind configurations, you can run the bitcoind node.
In the command below, use the path to your `bitcoin.conf` file from your machine and run the command in the terminal.

```bash
$ ./bitcoind -conf=<path-to-bitcoin.config>/bitcoin.conf
```
Once the above command is running, you will see `zmq_url` entries in the output, ensuring that zeromq is enabled.

### Configure chainhook

In this section, you will configure the chainhook and generate a `chainhook.toml` file to connect the chainhook with your bitcoind node. Navigate to the directory where you want to generate `chainhook.toml` file and use the following command in your terminal.

```bash
$ chainhook config generate --mainnet
```

You'll see that chainhook.toml file is generated.

In the `chainhook.toml` file that gets generated, you'll need to match the network parameters to the `bitcoin.config` that was generated earlier in [this](#prepare-the-bitcoind-node) section.

Now, in the `chainhook.toml`, update the following network parameters to use the bitcoind username, password and the network port from `bitcoin.conf`.

The Bitcoin node is exposing the rpc endpoints. In order to protect the endpoints, we are using rpc username and password fields. To run chainhooks as a service using stacks, we need to match the rpc endpoints username, password, and network ports.

Make sure the following configurations match before you proceed further in this article.

| bitcoin.conf      | chainhook.toml                |
| -----------       | -----------                   |
| rpcuser           | bitcoind_rpc_username         |
| rpcpassword       | bitcoind_rpc_password         |
| rpcport           | bitcoind_rpc_url              |
| zmqpubhashblock   | bitcoind_zmq_url              |

- Update the `bitcoind_rpc_username` to use the username set for `rpcuser` earlier.
- Update the `bitcoind_rpc_password` to use the password set for  `rpcpassword` earlier.
- Update the `bitcoind_rpc_url` to use the same host and port for the `rpcport` earlier.
- Next, update the `bitcoind_zmq_url` to use the same host and port for the `zmqpubhashblock` that was set earlier.

> [!NOTE]
> The `bitcoind_zmq_url` is optional when running chainhooks as a service using stacks because stacks will pull the blocks from both stacks and the Bitcoin chain.

The generated file `chainhook.toml` looks like this:

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
bitcoind_rpc_url = "http://localhost:8332"   --> Must match the rpcport in the bitcoin.conf
bitcoind_rpc_username = "<bitcoind_username>"             --> Must match the rpcuser in the bitcoin.conf
bitcoind_rpc_password = "<bitcoind_password>"             --> Must match the rpcpassword in the bitcoin.conf
stacks_node_rpc_url = "http://localhost:20443"

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

## Scan the blocks with predicate

Now that your configurations are done, you can scan your blocks by defining your predicates. This section helps you with an example JSON file to scan a range of blocks in the blockchain and render the results. To understand the supported predicates for Bitcoin, refer to [how to use chainhook with bitcoin](how-to-use-chainhook-with-bitcoin.md).

You will follow the steps below to scan blocks based on the predicates defined.

- Define the json file with your predicates
- Use the chainhook scan command to generate output.

### Define the predicates

This section walks you through the `ordinals.json` as an example to scan a range of blocks. You can use this file as a reference to create a JSON file with your predicates.

Create a new file, `ordinals.json`, in your project directory and paste the following code into it.

In the predicate definition below, the blocks are generated to a text file `inscription_feed.txt`.

```json
{
    "uuid": "1",
    "name": "Hello Ordinals",
    "chain": "bitcoin",
    "version": 1,
    "networks": {
        "mainnet": {
            "start_block": 777534,
            "end_block": 777540,
            "if_this": {
                "scope": "ordinals_protocol",
                "operation": "inscription_feed"
            },
            "then_that": {
                "file_append": {
                    "path": "inscription_feed.txt"
                }
            }
        }

    }
}
```

### Scan predicates using chainhook

Now, use the following command to scan the blocks based on the predicates defined in the `ordinals.json` file.

``` bash
$ chainhook predicates scan ordinals.json --config-path=./Chainhook.toml
```

>[!NOTE]
> You can get blockchain height and current block by referring to https://explorer.hiro.so/blocks?chain=mainnet

The above command will generate a new file, `mainnet-inscription_feed.txt,` in your directory with the results of the blocks based on the predicates.

Also, note that the above JSON file uses the `file_append` function in the `then-that` predicate design to output the scanned blocks into a file.

> [!TIP]
> To optimize your experience with scanning, the following are a few knobs you can play with:
> - Use of adequate values for `start_block` and `end_block` in predicates will drastically improve the speed.
> - Networking: reducing the number of network hops between the chainhook and the bitcoind processes can also help.


## Initiate chainhook service with predicate

In this section, you'll initiate the chainhook service and use the REST API call to post the events onto a local server.

The following is an example of predicate design where you define the start block with a scope and let the chainhook post the events to the local server.

In your terminal, navigate to your project directory and run the command below to generate a sample JSON file with predicates.

`$ chainhook predicates new ordinals_protocol.json --bitcoin`

A JSON file `ordinals_protocol.json` is generated. You can now edit the JSON based on the available predicates for Bitcoin. To understand the available predicates, refer to [how to use chainhook with bitcoin](how-to-use-chainhook-with-bitcoin.md).

```json
{
    "uuid": "1",
    "name": "Hello Ordinals",
    "chain": "bitcoin",
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
                    "authorization_header": "123904"
                }
            }
        }

    }
}
```

Once you have your JSON ready with the predicates, you can now start the chainhook service by passing the path to your JSON file.

``` bash
$ chainhook service start --predicate-path=ordinals_protocol.json --config-path=./Chainhook.toml
```

> [!NOTE]
> The `--predicate-path` and  `--config-path` flags are always the path to your predicate definition JSON file and the chainhook configuration file on your machine.

While the chainhook service runs, you can dynamically add more predicates or update your predicates. You can do this by

- enabling the http_port to send events dynamically.
  - Uncomment these lines of code in your JSON file.
    - ```
        [http_api]
        http_port = 20456
        database_uri = "redis://localhost:6379/"
        ```

In this case, `chainhook` will post payloads to `http://localhost:6379/events`. The following is a sample payload response.

```jsonc
{
    "chainhook": {
        "predicate": {
            "operation": "inscription_feed",
            "scope": "ordinals_protocol"
        },
        "uuid": "1"
    },
    "apply": [{
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
        "transactions": [{
            "transaction_identifier": {
                "hash": "0xca20efe5e4d71c16cd9b8dfe4d969efdd225ef0a26136a6a4409cb3afb2e013e"
            },
            "metadata": {
                "ordinal_operations": [{
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
                }],
                "proof": null
            },
            "operations": []
            // Other transactions
        }]
    }],
    "rollback": [],
}
```
Understand the output of the above JSON file with the following details.

- The `apply` payload includes the block header and the transactions that triggered the predicate.

- The `rollback` payload includes the block header and the transactions that triggered the predicate for a past block that is no longer part of the canonical chain and must be reverted.

## References

To learn more about Ordinals, refer to [Introducing Ordinals Explorer and Ordinals API](https://www.hiro.so/blog/introducing-the-ordinals-explorer-and-ordinals-api)
