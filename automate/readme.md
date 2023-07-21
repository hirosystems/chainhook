# automate-chainhooks
automate-chainhooks is a node script for running the chainhooks predicate.

### Prerequisites

- npm installed and Node v16.*
- [ngrok](https://dev.to/ibrarturi/how-to-test-webhooks-on-your-localhost-3b4f)
- [chainhook](https://github.com/hirosystems/chainhook/blob/5791379655fba786abf6e265311c0d789a8722e5/docs/getting-started.md)

### Run script
1. Go to the root of the project and do `npm install`. Make sure you have satisfied the above Prerequisites.
2. Start ngrok using the command `ngrok http 127.0.0.1:3009`. Once it starts, provide the ngrok URL in the `.env` file for `DOMAIN_URL`. This is required to post the result for the http predicates. You can check the ngrok requests at `localhost:4040`
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