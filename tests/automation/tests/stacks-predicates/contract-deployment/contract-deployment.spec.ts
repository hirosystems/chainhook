import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import { getPOSTPage } from "../../utility/browser-instance";
import predicateCommands from "../../stacks-predicates/predicate-commands.json";
import { PostPageInstance } from "../../utility/post-page-instance";
const expectedIdentifier = "ST20X3DC5R091J8B6YPQT638J8NR1W83KN6JQ4P6F";

jest.setTimeout(45 * 60 * 1000); // 45 mins

describe("contract-deployment:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for Contract Deployment");
    await contractDeploymentFilePredicate();
    console.log("COMPLETED file-append predicate for Contract Deployment");
    const result = await contractDeploymentFileResult();
    const actualIdentifier = result.apply[0]?.transactions[0]?.metadata?.kind?.data?.contract_identifier;
    const actualType = result.apply[0]?.transactions[0]?.metadata?.kind?.type;
    expect(actualIdentifier).toContain(expectedIdentifier);
    expect(actualType).toEqual("ContractDeployment");
  });

  it("post test", async () => {
    console.log("EXECUTING post predicate for Contract Deployment");
    const { stdout, stderr } = await exec(predicateCommands.transaction_post);
    console.log(stderr);
    console.log("COMPLETED post predicate for Contract Deployment");
    // get the POST page from the browser
    const postPage: PostPageInstance = await getPOSTPage();
    const result = await postPage.getPOSTResult();

    const actualIdentifier = result.apply[0]?.transactions[0]?.metadata?.kind?.data?.contract_identifier;
    const actualType = result.apply[0]?.transactions[0]?.metadata?.kind?.type;
    expect(actualIdentifier).toContain(expectedIdentifier);
    expect(actualType).toEqual("ContractDeployment");
  });
});



const contractDeploymentFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.contract_deployment_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.contract_deployment_file.command
  );
  console.log(stderr);
};

const contractDeploymentFileResult = async (): Promise<any> => {
  let fileContent = fs.readFileSync(
    predicateCommands.contract_deployment_file.result_file,
    "utf8"
  );
  if (fileContent) {
    fileContent = JSON.parse(fileContent);
  }
  return fileContent;
};
