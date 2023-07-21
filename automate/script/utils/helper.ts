import * as fs from "fs";
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
export const initDomainAPI = async (): Promise<void> => {
  initTestnetStacksPOSTURL();
};

const initTestnetStacksPOSTURL = () => {
  const domainURL = process.env.DOMAIN_URL;
  if (!domainURL) {
    console.log("Please set the domain url for POST predicates");
    process.exit(1);
  }

  stacksPostFile.map((postFile) => {
    const fileContent = JSON.parse(
      fs.readFileSync(`script/stacks-predicates/${postFile}-post.json`, "utf8")
    );
    fileContent.networks.testnet.then_that.http_post.url = domainURL;
    fs.writeFileSync(`script/stacks-predicates/${postFile}-post.json`, JSON.stringify(fileContent));
  });
};

export const clearResultFiles = () => {
  stacksPostFile.map((postFile) => {
    fs.writeFileSync(`script/stacks-predicates/${postFile}-file.result.json`, '');
  });
};
