# automate-chainhooks
automate-chainhooks is a npm script for automation testing of [chainhook](https://github.com/hirosystems/chainhook/blob/5791379655fba786abf6e265311c0d789a8722e5/docs/getting-started.md)

### Prerequisites
- npm installed and Node v16.*
- [ngrok](https://dev.to/ibrarturi/how-to-test-webhooks-on-your-localhost-3b4f)
- [chainhook](https://github.com/hirosystems/chainhook/blob/5791379655fba786abf6e265311c0d789a8722e5/docs/getting-started.md)
- [zeromq] (https://zeromq.org/download/)


### Run script
1. Go to the root of the project and do `npm install`. Make sure you have satisfied the above Prerequisites.
2. Start ngrok using the command `ngrok http 3006`. Once it starts, copy the ngrok URL into the `.env` file for `DOMAIN_URL`. This is required to post the result for the http predicates example of the ngrok URL https://1f67-37-19-198-81.ngrok.io/. You can check the ngrok requests at `localhost:4040`
3. Run all the predicates:
    ```sh
    $ npm run predicates
4. Run all the file result predicates:
    ```sh   
    $ npm run file-predicates
5. Run all the POST URL predicates:
    ```sh
    $ npm run post-predicates
6. Clear all the result files:
    ```sh
    $ npm run clear-result-files
7. Run transaction predicate for file append and POST
    ```sh
    $ predicate=transaction npm run predicates
8. Run print-event predicate for file append and POST
    ```sh
    $ predicate=print-event npm run predicates
9. Run nft-event predicate for file append and POST
    ```sh
    $ predicate=nft-event npm run predicates
10. Run ft-event predicate for file append and POST
    ```sh
    $ predicate=ft-event npm run predicates
11. Run contract-deployment predicate for file append and POST
    ```sh
    $ predicate=contract-deployment npm run predicates
12. Run contract-call predicate for file append and POST
    ```sh
    $ predicate=contract-call npm run predicates
13. Run block-height predicate for file append and POST
    ```sh
    $ predicate=block-height npm run predicates
14. Run stx-event predicate for file append and POST
    ```sh
    $ predicate=stx-event npm run predicates


### Bitcoin
Run the bitcoind with `./bitcoind -rpcuser=root -rpcpassword=root`. Set this user and root in `Chainhook.toml` file and run the command as `chainhook predicates scan /home/user/tests/stacks-predicates/transaction/transaction-bitcoin-file.json --config-path=./Chainhook.toml`
