---
title: Run Chainhook as a Service using Bitcoind
---

You can run Chainhook as a service to evaluate Bitcoin against your predicates. You can also dynamically register new predicates by enabling predicates registration API.

## Prerequisites

### Setting up a Bitcoin Node

Bitcoind is a program that implements the Bitcoin protocol for remote procedure call (RPC) use. Chainhook can be set up to interact with the Bitcoin chainstate through bitcoind's ZeroMQ interface, its embedded networking library.

This guide is written to work with the latest Bitcoin Core software containing bitcoind, [Bitcoin Core 25.0](https://bitcoincore.org/bin/bitcoin-core-25.0/).

> **_NOTE:_**
>
> While bitcoind can and will start syncing a Bitcoin node, customizing this node to your use cases beyond supporting a Chainhook is out of scope for this guide. See the Bitcoin wiki for ["Running Bitcoin"](https://en.bitcoin.it/wiki/Running_Bitcoin) or bitcoin.org's [Running A Full Node guide](https://bitcoin.org/en/full-node).

- Navigate to your project folder, create a new file, and rename it to `bitcoin.conf` on your local machine. Copy the below configuration to the `bitcoin.conf` file.
- The Chainhook will scan against bitcoin blockchain data. Copy the path of your Bitcoin directory to the `bitcoin.conf`'s `datadir` field. See the Bitcoin wiki for the [list of default directories by operating system](https://en.bitcoin.it/wiki/Data_directory)
- Set a username of your choice for bitcoind and use it in the `rpcuser` configuration below.
- Set a password of your choice for bitcoind and use it in the `rpcpassword` configuration below.

> **_NOTE:_**
>
> Make a note of the `rpcuser`, `rpcpassword` and `rpcport` values to use them later in the chainhook configuration.

```conf
# Bitcoin Core Configuration

datadir=</path/to/bitcoin/directory/> # Path to existing Bitcoin folder. New data directory will be created here otherwise
server=1
rpcuser=devnet  # You can set the username here
rpcpassword=devnet  #  You can set the password here
rpcport=8332   # You can set your localhost port number here
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

Now that you have `bitcoin.conf` file ready with the bitcoind configurations, you can run the bitcoind node.
In the command below, use the path to your `bitcoin.conf` file from your machine and run the command in the terminal.

> **_NOTE:_**
>
> The below command is a startup process that, if this is your first time syncing a node, might take a few hours to a few days to run. Alternatively, if the directory pointed to in the `datadir` field above contains bitcoin blockchain data, syncing will resume.

```console
./bitcoind -conf=<path-to-bitcoin.conf>
```

Once the above command runs, you will see `zmq_url` entries in the output, enabling ZeroMQ.

### Configure Chainhook

In this section, you will configure chainhook to match the network configurations with the bitcoin config file. First, [install the latest version of chainhook](../getting-started.md#install-chainhook-from-source).

Next, you will generate a `Chainhook.toml` file to connect Chainhook with your bitcoind node. Navigate to the directory where you want to generate the `Chainhook.toml` file and use the following command in your terminal:

```console
chainhook config generate --mainnet
```

The following `Chainhook.toml` file should be generated:

```toml
[storage]
working_dir = "cache"

# The Http Api allows you to register/deregister
# dynamically predicates.
# Disable by default.
#
[http_api]
http_port = 20456
database_uri = "redis://localhost:6379/"

[network]
mode = "testnet"
bitcoind_rpc_url = "http://localhost:8332" # Must match the rpcport in the bitcoin.conf
bitcoind_rpc_username = "<bitcoind_username>" # Must match the rpcuser in the bitcoin.conf
bitcoind_rpc_password = "<bitcoind_password>" # Must match the rpcpassword in the bitcoin.conf
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

Several of the network parameters in the generated `Chainhook.toml` configuration file need to match the network parameters contained in the `bitcoin.conf` that was created earlier in the [Setting up a Bitcoin Node](#setting-up-a-bitcoin-node) section:

- Update the `bitcoind_rpc_username` to use the username set for `rpcuser` earlier.
- Update the `bitcoind_rpc_password` to use the password set for `rpcpassword` earlier.
- Update the `bitcoind_rpc_url` to use the same host and port for the `rpcport` earlier.
- Next, update the `bitcoind_zmq_url` to use the same host and port for the `zmqpubhashblock` that was set earlier.

| bitcoin.conf    | Chainhook.toml        |
| --------------- | --------------------- |
| rpcuser         | bitcoind_rpc_username |
| rpcpassword     | bitcoind_rpc_password |
| rpcport         | bitcoind_rpc_url      |
| zmqpubhashblock | bitcoind_zmq_url      |



## Scan blockchain based on predicates

Now that your bitcoind and Chainhook configurations are complete, you can define the [predicates](../overview.md#if-this-predicate-design) you would like to scan against bitcoin blocks [predicates](../overview.md#if-this-predicate-design). These predicates are where the user specifies the kinds of blockchain events they want their Chainhook to trigger an action. This section helps you with an example JSON file to scan a range of blocks in the blockchain to trigger results. To understand the supported predicates for Bitcoin, refer to [how to use chainhooks with bitcoin](how-to-use-chainhooks-with-bitcoin.md).

The following is an example to walk you through `file_append` `then-that` predicate design.

The example collects bitcoin transactions from a particular address, specifically the bitcoin address associated with a Stacking pool, [Friedgar Pool](https://pool.friedger.de/). This address has been collecting payouts from Stacks miners since cycle 55. We are scanning a portion of the bitcoin blockchain to capture the last few of these payouts (to shorten predicate scanning for example purposes).

### Example 1

Run the following command in your terminal to generate a sample JSON file with predicates.

```console
touch stacking.json
```

Paste the following contents into the `stacking.json` file. 

```json
{
  "chain": "bitcoin",
  "uuid": "13b3fce6-eace-4552-a2f6-7672cd94cf7e",
  "name": "Friedgar's Stacking Pool",
  "version": 1,
  "networks": {
    "mainnet": {
      "start_block": 800000,
      "end_block": 802000,
      "if_this": {
        "scope": "outputs",
        "p2wpkh": {
          "equals": "bc1qs0kkdpsrzh3ngqgth7mkavlwlzr7lms2zv3wxe"
      }
    },
      "then_that": {
        "file_append": {
          "path": "btc-transactions.txt"
        }
      }
    }
  }
}
```

> **_NOTE:_**
>
> You can get blockchain height and current block by referring to https://explorer.hiro.so/blocks?chain=mainnet

Now, use the following command to scan the blocks based on the predicates defined in the `stacking.json` file.

```console
chainhook predicates scan stacking.json --config-path=./Chainhook.toml
```

The output of the above command will be a text file `btc-transactions.txt` generated based on the predicate definition.

> **_TIP:_**
>
> To optimize your experience with scanning, the following are a few knobs you can play with:
>
> - Use of adequate values for `start_block` and `end_block` in predicates will drastically improve the performance.
> - Reducing the number of network hops between the Chainhook and the bitcoind processes can also help, so your network setup can play a major role in performance.


## References

- To learn more about Ordinals, refer to [Introducing Ordinals Explorer and Ordinals API](https://www.hiro.so/blog/introducing-the-ordinals-explorer-and-ordinals-api).
- The [OpenAPI specification for chainhook](https://raw.githubusercontent.com/hirosystems/chainhook/develop/docs/chainhook-openapi.json) is available to understand the scope of chainhook.
