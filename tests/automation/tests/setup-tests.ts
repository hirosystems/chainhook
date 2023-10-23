import * as dotenv from 'dotenv';
import * as fs from "fs";
import express from 'express';

const app: express.Application = express();
const port: number = 3006;

dotenv.config({ path: '.env' });

const stacksPostFile = [
  "transaction/transaction",
  "block-height/block-height",
  "ft-event/ft-event",
  "nft-event/nft-event",
  "stx-event/stx-event",
  "print-event/print-event",
  "contract-call/contract-call",
  "contract-deployment/contract-deployment"
];

const initTestnetStacksPOSTURL = () => {
  const domainURL = process.env.DOMAIN_URL;
  if (!domainURL) {
    console.log("Please set the domain url for POST predicates");
    process.exit(1);
  }

  stacksPostFile.map((postFile) => {
    const fileContent = JSON.parse(
      fs.readFileSync(`tests/stacks-predicates/${postFile}-post.json`, "utf8")
    );
    fileContent.networks.testnet.then_that.http_post.url = domainURL;
    fs.writeFileSync(`tests/stacks-predicates/${postFile}-post.json`, JSON.stringify(fileContent));
  });
};

initTestnetStacksPOSTURL();

// register routes for POST predicate and start server
app.get('*', (_req, _res) => {
  console.log(`Invoked GET at http://localhost:${port}/`);
  _res.status(200).send('acknowledge');
});

app.post('*', (_req, _res) => {
  console.log(`Invoked POST at http://localhost:${port}/`);
  _res.status(200).send('acknowledge');
});

// Server setup
app.listen(port, () => {
  console.log(`Listening at http://localhost:${port}/`);
});