import { getPOSTPage } from "../../utility/browser-instance";
import predicateCommands from "../../stacks-predicates/predicate-commands.json";
import { PostPageInstance } from "../../utility/post-page-instance";

import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
const expectedTxId = "0x411e78f4b727fc0a78b86c3fd56da0c741c71339713be81d7528c4015665267b";

jest.setTimeout(45 * 60 * 1000); // 45 mins

describe("Transaction:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for transaction");
    await transactionFilePredicate();
    console.log("COMPLETED file-append predicate for transaction");
    const result = await transactionFileResult();
    const actualTxId = result.apply[0]?.transactions[0]?.transaction_identifier.hash;
    expect(actualTxId).toEqual(expectedTxId);
  });

  it("post test", async () => {
    console.log("EXECUTING post predicate for transaction");
    const { stdout, stderr } = await exec(predicateCommands.transaction_post);
    console.log(stderr);
    console.log("COMPLETED post predicate for transaction");
    // get the POST page from the browser
    const postPage: PostPageInstance = await getPOSTPage();
    const result = await postPage.getPOSTResult();
    const actualTxId = result.apply[0]?.transactions[0]?.transaction_identifier.hash;
    expect(actualTxId).toEqual(expectedTxId);
    await postPage.closeBrowser();
  });
});

const transactionFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.transaction_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.transaction_file.command
  );
  console.log(stderr);
};

const transactionFileResult = async (): Promise<any> => {
  const fileContent = JSON.parse(
    fs.readFileSync(predicateCommands.transaction_file.result_file, "utf8")
  );
  return fileContent;
};
