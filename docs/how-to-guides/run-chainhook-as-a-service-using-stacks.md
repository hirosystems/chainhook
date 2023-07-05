---
title: Run Chainhook as a service using Stacks
---

# Run Chainhook as a service using Stacks

The following document helps you with the steps to run chainhooks as a service using Stacks. By the end of this document, you will have your chainhook communicating with the Stacks and Bitcoin layers of the blockchain to retrieve results based on the predicates.

You can start with the prerequisite section and configure your files to start the chainhook service.

## Prerequisite

- Configure your stacks node by referring to the [Stacks node configuration](https://docs.stacks.co/docs/nodes-and-miners/stacks-node-configuration) document.
- Recommend the latest version of Stacks. You can check the latest version by following [this](https://github.com/stacks-network/stacks-blockchain/releases) link.
- Get the rpcuser, rpcpassword, and rpc_port defined in the bitcoin.conf file from [this section](run-chainhook-as-a-service-using-bitcoind.md#prepare-the-bitcoind-node) to use in this article.

A `stacks.toml` file gets generated when you configure the stacks node, as shown below. Ensure that the `username`, `password`, and `rpc_port` values match the values in the `bitcoin.conf` file. Also, note the `rpc_bind` port to use in the `chainhook.toml` configuration in the next section of this article.

```
[node]
working_dir = "/stacks-blockchain"
rpc_bind = "0.0.0.0:20443"          --> Make a note of this port to use in the `chainhook.toml`
p2p_bind = "0.0.0.0:20444"
bootstrap_node = "02da7a464ac770ae8337a343670778b93410f2f3fef6bea98dd1c3e9224459d36b@seed-0.mainnet.stacks.co:20444,02afeae522aab5f8c99a00ddf75fbcb4a641e052dd48836408d9cf437344b63516@seed-1.mainnet.stacks.co:20444,03652212ea76be0ed4cd83a25c06e57819993029a7b9999f7d63c36340b34a4e62@seed-2.mainnet.stacks.co:20444"

[burnchain]
chain = "bitcoin"
mode = "mainnet"
peer_host = "localhost"
username = "bitcoind_username"       --> Must match with the rpcuser in the bitcoin.conf
password = "bitcoind_password"       --> Must match with the rpcpassword in the bitcoin.conf
rpc_port = 8332                      --> Must match with the rpcport in the bitcoin.conf
peer_port = 8333

[[events_observer]]
endpoint = "localhost:20455"
retry_count = 255
events_keys = ["*"]

```

### Configure Chainhook

In this section, you will configure the chainhook to communicate with the network using the following command. Run the following command in your terminal and generate the `chainhook.toml` file.

```bash
$ chainhook config generate --mainnet
```

Below is the generated `chainhook.toml` file. Ensure that the `bitcoind_rpc_url`, `bitcoind_rpc_username`, `bitcoind_rpc_password` are matching with the `rpcport`, `rpcuser` and `rpcpassword` from `bitcoin.conf` file and the port of the `stacks_node_rpc_url` matches the `rpc_bind` in the `Stacks.toml` file.

```toml
[storage]
working_dir = "cache"

# The Http Api allows you to register/deregister
# dynamically predicates.
# Disable by default.
#
# [http_api]
# http_port = 20456
# database_uri = "redis://localhost:6379/"

[network]
mode = "mainnet"
bitcoind_rpc_url = "http://localhost:8332"                --> Must match with the rpcport in the bitcoin.conf
bitcoind_rpc_username = "<bitcoind_username>"             --> Must match with the rpcuser in the bitcoin.conf
bitcoind_rpc_password = "<bitcoind_password>"             --> Must match with the rpcpassword in the bitcoin.conf
stacks_node_rpc_url = "http://localhost:20443"            --> Must match with the rpc_bind in the Stacks.toml file

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

Now, in order to have Chainhook communicating with Stacks and Bitcoin layers, we need to have the following configurations matched.


| bitcoin.conf      | stacks.toml            | chainhook.toml               |
| -----------       | -----------            |  ----------- 
| rpcuser           | username               | bitcoind_rpc_username        |
| rpcpassword       | password               | bitcoind_rpc_password        |
| rpcport           | rpc_port               | bitcoind_rpc_url             |
| zmqpubhashblock   |                        | bitcoind_zmq_url             |
|                   | rpc_bind               | stacks_node_rpc_url          |
|                   | endpoint               | stacks_events_ingestion_port |

> [!NOTE]
> The `bitcoind_zmq_url` is optional when running chainhook as a service using stacks because stacks will pull the blocks from both stacks and the Bitcoin chain.

Once you have all the above configurations matched, you can start your chainhook service by running the following command:

## Scan the blocks with predicate

Now that your configurations are done, you can scan your blocks by defining predicates. This section helps you with an example JSON file to scan a range of blocks in the blockchain and render the results. To understand the supported predicates for Stacks, refer to [how to use chainhook with Stacks](how-to-use-chainhook-with-stacks.md).

You will follow the steps below to scan blocks based on the predicates defined.

- Define the JSON file with your predicates
- Use the chainhook scan command to generate output

### Define the predicates

This section walks you through the `hello-arkadiko.json` as an example to scan a range of blocks. You can use this file as a reference to create a JSON file or update the sample file with your predicates.

In this section, you can use the following command to generate a sample JSON with predicates.

`$ chainhook predicates new hello-arkadiko.json --stacks`

The above command generates a `hello-arkadiko.json` file in your directory. Below is a sample of the file. 
You can update the below JSON file based on the [available predicates for Stacks](how-to-use-chainhook-with-stacks.md). To understand the current block height to scan between a range of blocks, you can look into the [Stacks Explorer](https://explorer.hiro.so/blocks?chain=mainnet).

```json
{
  "chain": "stacks",
  "uuid": "1da35032-e399-430c-bfbc-eca94709ad11",
  "name": "Hello world",
  "version": 1,
  "networks": {
    "testnet": {
      "start_block": 0,
      "end_block": 100,
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
      "start_block": 0,
      "end_block": 100,
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

Note that the above example uses `file-append` as the `then-that` predicate, which means that the output is appended or generated to a file mentioned in the path of the file_append function above. In the above example, a new file, `arkadiko.txt,` is created, and the output of the predicate scan is appended to this file.

*****Add sample output -- Ludo*********

Another example of the predicate to post events to a server:

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

Note that the above example uses `HTTP-post` as the `then-that` predicate, which means that the output is posted to a local server based on the port mentioned in the URL. You can use the URL in the HTTP-post function to scan the events.

### Scan predicates using chainhook

Now that your predicates are defined, you can scan the blockchain using chainhook.

Use the following command to scan the blocks based on the predicates defined in the `hello-arkadiko.json` file.

> [!Warning]
> When the above command runs for the first time, a chainstate archive will be downloaded, uncompressed, and written to the disk. The approximate size required for the disk is stated below:
> - 3GB required for the testnet 
> - 10GB for the mainnet
> The subsequent scans will use the cached chainstate if already present, speeding up iterations and the overall feedback loop.

```bash
$ chainhook predicates scan hello-arkadiko.json --mainnet
```

> [!NOTE]
> The chainhook will select the predicate based on the network passed in the above command. If you wish to run the predicate scan for testnet, use the following command:
>  ```bash
> $ chainhook predicates scan hello-arkadiko.json --testnet
> ```

## Initiate chainhook service

> [!NOTE]
> The `--predicate-path` and  `--config-path` flags are always the path to your predicate definition JSON file and the chainhook configuration file on your machine.

In this section, you'll learn how to initiate chainhook service. There are three ways to do this:

1. Pass the JSON file path with predicates to the command below and run the command to start the chainhook service.

   1. `chainhook service start --predicate-path=hello-arkadiko.json --config-path=chainhook.toml`

2. Run the command below and dynamically pass your predicates as a JSON during the API call.

   1. `$ chainhook service start --config-path=chainhook.toml`

    You can dynamically register the predicate during the API call. Ensure the port number `http_port = 20456` matches the `chainhook.toml` file.

3. You can initiate API service while the chainhook service starts by using the following command:

   1. `$ chainhook service start --predicate-path=hello-arkadiko.json --start-http-api --config-path=chainhook.toml`

> [!NOTE]
> You can define multiple predicates and pass them as arguments to start the chainhook service. 
> Example:  `$ chainhook service start --predicate-path=hello-arkadiko.json --predicate-path=hello-arkadiko.json --start-http-api --config-path=chainhook.toml`

