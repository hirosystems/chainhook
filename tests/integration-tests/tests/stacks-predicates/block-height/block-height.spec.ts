import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import predicateCommands from "../../stacks-predicates/predicate-commands.json";

jest.setTimeout(45 * 60 * 1000); // 45 mins

// TODO: Ask that this this block height: 114260 brings all the data which is very huge 
describe("block-height:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for Block Height");
    await blockHeightFilePredicate();
    console.log("COMPLETED file-append predicate for Block Height");
    const result = await blockHeightFileResult();
    expect(0).toEqual(1);
  });
});

const blockHeightFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.block_height_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.block_height_file.command
  );
  console.log(stderr);
};

const blockHeightFileResult = async (): Promise<any> => {
  let fileContent = fs.readFileSync(
    predicateCommands.block_height_file.result_file,
    "utf8"
  );
  if (fileContent) {
    fileContent = JSON.parse(fileContent);
  }
  return fileContent;
};
