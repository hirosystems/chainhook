---
title: Getting Started
---

# Getting Started

Chainhooks interact with Bitcoin and Stacks layers. You can use chainhooks as a tool in your local environment or as a service in the cloud environment. 

- Chainhooks as a tool
- Chainhooks as a service

## Chainhooks as a tool

Chainhooks can be run as a tool in your local environment by understanding the steps below. Chainhooks are often described using the term predicates, which are defined as conditions to scan your blocks on the blockchain.

1. Define predicates
2. Test/Scan predicates
3. Deploy predicates

Note that the above three steps are common for running chainhooks as a service using both Stacks and Bitcoin.

Once you are ready to run chainhooks as a tool in your local environment, you can install chainhooks by following the next section.

### Install Chainhooks from the source

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

Next, you can define your predicates by following the [how to use chainhook with bitcoin](how-to-use-chainhook-with-bitcoin.md) and [how to use chainhook with stacks](how-to-use-chainhook-with-stacks.md).

## Chainhooks as a service

You can run chainhooks as a service in your cloud environment by passing the path of the predicates in a JSON format or by defining the predicates dynamically.

You can also define the predicates dynamically using two options.

- Use the *predicate json file path* while starting the chainhook service.
- Use the *start-http-api* option to instantiate a REST API allowing developers to list, add, and remove predicates at runtime.

Now, if you decide to use chainhooks as a tool in your local environment, you can start installing chainhooks from the source.
