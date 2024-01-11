import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import predicateCommands from "../../stacks-predicates/predicate-commands.json";

jest.setTimeout(30 * 60 * 1000); // 30 mins

// TODO: Ask that the expire_after_occurrence: 1 does not work but in readme it says it work
describe("stx-event:", () => {
  it("file-append test", async () => {
    console.log("EXECUTING file-append predicate for STX Event");
    await stxEventFilePredicate();
    console.log("COMPLETED file-append predicate for STX Event");
    const result = await stxEventFileResult();
    expect(0).toEqual(1);
  });
});

const stxEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.stx_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.stx_event_file.command
  );
  console.log(stderr);
};

const stxEventFileResult = async (): Promise<any> => {
  let fileContent = fs.readFileSync(
    predicateCommands.stx_event_file.result_file,
    "utf8"
  );
  if (fileContent) {
    fileContent = JSON.parse(fileContent);
  }
  return fileContent;
};
