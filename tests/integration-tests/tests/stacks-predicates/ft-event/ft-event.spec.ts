import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import predicateCommands from "../../stacks-predicates/predicate-commands.json";

jest.setTimeout(30 * 60 * 1000); // 30 mins

// TODO: Ask that this asset_identifier: ST113MYNN52BC76GWP8P9PYFEP7XWJP6S5YFQM4ZE.shitty-coin::shitty brings all the blocks
describe("ft-event:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for FT Event");
    await FTEventFilePredicate();
    console.log("COMPLETED file-append predicate for FT Event");
    const result = await FTEventFileResult();
    expect(0).toEqual(1);
  });
});

const FTEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.ft_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.ft_event_file.command
  );
  console.log(stderr);
};

const FTEventFileResult = async (): Promise<any> => {
  let fileContent = fs.readFileSync(
    predicateCommands.ft_event_file.result_file,
    "utf8"
  );
  if (fileContent) {
    fileContent = JSON.parse(fileContent);
  }
  return fileContent;
};
