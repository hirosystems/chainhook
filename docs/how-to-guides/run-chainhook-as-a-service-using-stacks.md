---
title: Run Chainhook as a service using Stacks
---

# Run Chainhook as a service using Stacks

The following document helps you with the steps to run chainhooks as a service using Stacks. You can start with the prerequisite section and then configure your files to start the chainhook service.

## Prerequisite

- Need stacks node running. You can refer to [Stacks node configuration](https://docs.stacks.co/docs/nodes-and-miners/stacks-node-configuration)
- Recommend the latest version of Stacks. You can check the latest version by following [this](https://github.com/stacks-network/stacks-blockchain/releases) link.
- Register event observer stacks.toml file (Refer to https://docs.stacks.co/docs/nodes-and-miners/stacks-node-configuration#events_observer)

When you configure stacks node, a `stacks.toml` that gets generated as shown in the [sample file](https://docs.stacks.co/docs/nodes-and-miners/stacks-node-configuration#example-mainnet-follower-configuration)

*******************Do we need to add the following config?************** to be added to `chainhook.toml` file.

test predicate on machine, use the link below. 

### Configure chainhook

In this section, you will configure chainhook using the following command, and generate a chainhook toml file. 

```bash
$ chainhook config generate --mainnet
```

Observe that the generated Chainhook config file has the following configuration.

```
[[event_source]]
tsv_file_url = "https://archive.hiro.so/mainnet/stacks-blockchain-api/mainnet-stacks-blockchain-api-latest"
```

In the `stacks.toml` file that gets generated, make sure the following matches with the bitcoind username, password and ports.  

```
username = "user"
password = "pass"
rpc_port = 8332
```

