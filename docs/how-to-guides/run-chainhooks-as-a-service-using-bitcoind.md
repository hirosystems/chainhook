---
title: Run Chainhook as a service using Bitcoind
---

# Run Chainhook as a service using Bitcoind

Write predicates - How to use chainhook with Bitcoin
    Note: Define this clearly - predicate.json
Test predicates - Scan command
    chainhook predicates scan ./path/to/predicate.json --mainnet
Deploy predicates 
    Platform, run chainhook service.

1. Install bitcoind using [this](https://bitcoin.org/en/bitcoin-core/) link.

```bash
brew install bitcoind
```

The above command downloads binaries and by default zeromq is enabled.

1. Check zeromq is enabled from bitcoind doc. 
****************How do we verify that zeromq is enabled?**************************


2. Prepare bitcoind node (downloading blocks)
    1. Config file - mainnet.config 
    2. datadir: Bitcoind downloaded path.
    3. Start zeromq
        1. Enabling zeromq in Bitcoind
        2. zmq_url to be matched with url - 18543

You will configure chainhook node by observing bitcoin chain through zeromq messages sent by bitcoin.

chainhook-node can get blocks from either:
- bitcoind


## Ingesting blocks through bitcoind directly

Happening through zeromq (make sure this is part of the bitcoind install).

Based on the logs, `zmqpubhashblock="tcp://0.0.0.0:18543"` indicates that zeromq is installed.

In the bitcoind document that is installed, make sure the port number in the `zmq_url` matches with the `zmqpubhashblock` parameter in the `mainnet.conf` file.

### Prepare the bitcoind node

Step 1: create a config file:
Typical config file `mainnet.conf` for `bitcoind`:

```conf
datadir=<path-to-your-downloaded-bitcoind>
server=1
rpcuser="bitcoind_username"  -->You can set the username here
rpcpassword="bitcoind_password"  -->You can set the password here
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

Step 2: run a bitcoind node:

```bash
$ bitcoind -conf=<path-to-mainnet.config>/mainnet.conf
```

Based on the logs, `zmqpubhashblock="tcp://0.0.0.0:18543"` indicates that zeromq is installed.

### Configure chainhook

In this section, you will configure chainhook using the following command, and generate a chainhook toml file. 

```bash
$ chainhook config generate --mainnet
```
In the `chainhook.toml` file that gets generated, you'll need to match some of the network parameters to the `mainnet.config` that was generated earlier in this article in [this](#prepare-the-bitcoind-node) section.

Now, in the `chainhook.toml`, update the following network parameters to use the same username, password and the network port.

- Update the `bitcoind_rpc_username` to use the username that was set for `rpcuser` earlier.
- Update the `bitcoind_rpc_password` to use the password that was set for  `rpcpassword` earlier.
- Update the `bitcoind_rpc_url` to use the same host and port for the `rpcport` earlier.
- Next, update the `bitcoind_zmq_url` to use the same host and port for the `zmqpubhashblock` that was set earlier.

********************We are only specifying the port in the rpcport but not the host. Do we need both?******

Bitcoind is going to generate a local host by default and no need of a port.

```toml
[storage]
working_dir = "cache" # Directory used by chainhook node for caching data

# The Http Api allows you to register / deregister
# dynamically predicates.
# Disable by default.
#
# [http_api]
# http_port = 20456
# database_uri = "redis://0.0.0.0:6379/"

[network]
mode = "mainnet"
bitcoind_rpc_url = "http://0.0.0.0:8332"    # mainnet.conf
bitcoind_rpc_username = "bitcoind_username" # mainnet.conf
bitcoind_rpc_password = "bitcoind_password" # mainnet.conf
bitcoind_zmq_url = "http://0.0.0.0:18543"   # mainnet.conf

[limits]
max_number_of_bitcoin_predicates = 100
max_number_of_concurrent_bitcoin_scans = 100
max_number_of_processing_threads = 16
max_number_of_networking_threads = 16
max_caching_memory_size_mb = 32000

```

#### Use case 1: Scan a few blocks

The following is an example to scan a range of blocks by defining the start and end blocks.

***************How can a user know the block height? Can we provide more details here?********************

For block height: refer to https://explorer.hiro.so/blocks?chain=mainnet
You can always define your own predicated by referring to the [configure bitcoin predicates](configure-bitcoin-predicates.md).

Assuming the following predicate `ordinals.json`: 
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
                    "path": "mainnet-inscription_feed.txt"
                }
            }
        }

    }
}
```

Now, use the following command to scan the blocks based on the predicates defined in the ordinals.json above.

``` bash
$ chainhook predicates scan ordinals.json --config-path=./Chainhook.toml
```

#### Use case 3: Stream blocks

In this scenario, you define the start block and then stream those blocks to post them as events.

If user want to stream blocks:

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

You can now start the chainhook service by using the following command:

``` bash
$ chainhook service start --predicate-path=ordinals.json --config-path=./Chainhook.toml
```

In this case, `chainhook` will be posting payloads to `http://localhost:3000/events`.
The following is an example payload in the form of a json file.

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
}```

The `apply` payload is including the block header and the transactions that triggered the predicate.

The `rollback` payload is including the block header and the transactions that triggered the predicate for a past block that is no longer part of the canonical chain, and that'd need to be reverted.

## Ingesting blocks through bitcoind

**********************What is happening here?*********************************

Local chainhook service/node is running.

Chainhook service running, tell chainhook node, there is a new block mined on the stacks chain.


Step 1: create a config file:
Typical config file `mainnet.conf` for `bitcoind`:

```
datadir=<PATH>
server=1
rpcuser=<USERNAME>        -->You can set the username here
rpcpassword=<PASSWORD>    -->You can set the password here
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
```

Step 2: Run a bitcoind node:

```
bitcoind -conf=<path-to-mainnet.conf>/mainnet.conf
```



TODO

- Ludo: document bitcoind config file
