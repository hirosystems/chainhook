import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import { getPOSTPage } from "../../utility/browser-instance";
import predicateCommands from "../../stacks-predicates/predicate-commands.json";
import { PostPageInstance } from "../../utility/post-page-instance";
const expectedIdentifier = "ST000000000000000000002AMW42H.bns";

jest.setTimeout(45 * 60 * 1000); // 45 mins

describe("contract-call:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for Contract Call");
    await contractCallFilePredicate();
    console.log("COMPLETED file-append predicate for Contract Call");
    const result = await contractCallFileResult();
    const actualIdentifier = result.apply[0]?.transactions[0]?.metadata?.kind?.data?.contract_identifier;
    const actualType = result.apply[0]?.transactions[0]?.metadata?.kind?.type;
    expect(actualIdentifier).toEqual(expectedIdentifier);
    expect(actualType).toEqual("ContractCall");
  });

  it("post test", async () => {
    console.log("EXECUTING post predicate for Contract Call");
    const { stdout, stderr } = await exec(predicateCommands.transaction_post);
    console.log(stderr);
    console.log("COMPLETED post predicate for Contract Call");
    // get the POST page from the browser
    const postPage: PostPageInstance = await getPOSTPage();
    const result = await postPage.getPOSTResult();
    const actualIdentifier = result.apply[0]?.transactions[0]?.metadata?.kind?.data?.contract_identifier;
    const actualType = result.apply[0]?.transactions[0]?.metadata?.kind?.type;
    expect(actualIdentifier).toEqual(expectedIdentifier);
    expect(actualType).toEqual("ContractCall");
  });

});

const contractCallFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.contract_call_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.contract_call_file.command
  );
  console.log(stderr);
};

const contractCallFileResult = async (): Promise<any> => {
  let fileContent = fs.readFileSync(
    predicateCommands.contract_call_file.result_file,
    "utf8"
  );
  if (fileContent) {
    fileContent = JSON.parse(fileContent);
  }
  return fileContent;
};
