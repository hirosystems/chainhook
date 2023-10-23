import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import predicateCommands from "../../stacks-predicates/predicate-commands.json";

jest.setTimeout(30 * 60 * 1000); // 30 mins

// TODO: Ask that this asset_identifier: ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.punker-nft3 does not have any match
describe("nft-event:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for NFT Event");
    await NFTEventFilePredicate();
    console.log("COMPLETED file-append predicate for NFT Event");
    const result = await NFTEventFileResult();
    expect(0).toEqual(1);
  });
});

const NFTEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.nft_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.nft_event_file.command
  );
  console.log(stderr);
};

const NFTEventFileResult = async (): Promise<any> => {
  let fileContent = fs.readFileSync(
    predicateCommands.nft_event_file.result_file,
    "utf8"
  );
  if (fileContent) {
    fileContent = JSON.parse(fileContent);
  }
  return fileContent;
};