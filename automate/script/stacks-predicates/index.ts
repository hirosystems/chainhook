import * as util from "util";
import * as fs from "fs";
import * as child_process from "child_process";
const exec = util.promisify(child_process.exec);
import predicateCommands from "./predicate-commands.json";

export const triggerAllPredicates = async () => {
  const selectedPredicate = process.env.predicate;
  if (!selectedPredicate) {
    await triggerTransaction();
    await triggerPrintEvent();
    await triggerNFTEvent();
    await triggerFTEvent();
    await triggerContractDeployment();
    await triggerContractCall();
    await triggerBlockHeight();
    await triggerSTXEvent();
  }
  if (selectedPredicate === "transaction") {
    await triggerTransaction();
  }
  if (selectedPredicate === "print-event") {
    await triggerPrintEvent();
  }
  if (selectedPredicate === "nft-event") {
    await triggerNFTEvent();
  }
  if (selectedPredicate === "ft-event") {
    await triggerFTEvent();
  }
  if (selectedPredicate === "contract-deployment") {
    await triggerContractDeployment();
  }
  if (selectedPredicate === "contract-call") {
    await triggerContractCall();
  }
  if (selectedPredicate === "block-height") {
    await triggerBlockHeight();
  }
  if (selectedPredicate === "stx-event") {
    await triggerSTXEvent();
  }
};

export const triggerAllFilePredicates = async () => {
  await transactionFilePredicate();
  await stxEventFilePredicate();
  await printEventFilePredicate();
  await NFTEventFilePredicate();
  await FTEventFilePredicate();
  await contractDeploymentFilePredicate();
  await contractCallFilePredicate();
  await blockHeightFilePredicate();
};

export const triggerAllPOSTPredicates = async () => {
  await transactionPOSTPredicate();
  await stxEventPOSTPredicate();
  await printEventPOSTPredicate();
  await NFTEventPOSTPredicate();
  await FTEventPOSTPredicate();
  await contractDeploymentPOSTPredicate();
  await contractCallPOSTPredicate();
  await blockHeightPOSTPredicate();
};

const triggerTransaction = async (): Promise<any> => {
  console.log("EXECUTING predicate for transaction");
  await transactionFilePredicate();
  await transactionPOSTPredicate();
  console.log("COMPLETED predicate for transaction");
};

const triggerSTXEvent = async (): Promise<any> => {
  console.log("EXECUTING predicate for STX Event");
  await stxEventFilePredicate();
  await stxEventPOSTPredicate();
  console.log("COMPLETED predicate for STX Event");
};

const triggerPrintEvent = async (): Promise<any> => {
  console.log("EXECUTING predicate for print Event");
  await printEventFilePredicate();
  await printEventPOSTPredicate();
  console.log("COMPLETED predicate for print Event");
};

const triggerNFTEvent = async (): Promise<any> => {
  console.log("EXECUTING predicate for NFT Event");
  await NFTEventFilePredicate();
  await NFTEventPOSTPredicate();
  console.log("COMPLETED predicate for NFT Event");
};

const triggerFTEvent = async (): Promise<any> => {
  console.log("EXECUTING predicate for FT Event");
  await FTEventFilePredicate();
  await FTEventPOSTPredicate();
  console.log("COMPLETED predicate for FT Event");
};

const triggerContractDeployment = async (): Promise<any> => {
  console.log("EXECUTING predicate for Contract Deployment");
  await contractDeploymentFilePredicate();
  await contractDeploymentPOSTPredicate();
  console.log("COMPLETED predicate for Contract Deployment");
};

const triggerContractCall = async (): Promise<any> => {
  console.log("EXECUTING predicate for Contract Call");
  await contractCallFilePredicate();
  await contractCallPOSTPredicate();
  console.log("COMPLETED predicate for Contract Call");
};

const triggerBlockHeight = async (): Promise<any> => {
  console.log("EXECUTING predicate for Block Height");
  await blockHeightFilePredicate();
  await blockHeightPOSTPredicate();
  console.log("COMPLETED predicate for Block Height");
};

const transactionFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.transaction_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.transaction_file.command
  );
  console.log(stderr);
};

const transactionPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.transaction_post);
  console.log(stderr);
};

const stxEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.stx_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.stx_event_file.command
  );
  console.log(stderr);
};

const stxEventPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.stx_event_post);
  console.log(stderr);
};

const printEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.print_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.print_event_file.command
  );
  console.log(stderr);
};

const printEventPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.print_event_post);
  console.log(stderr);
};

const NFTEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.nft_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.nft_event_file.command
  );
  console.log(stderr);
};

const NFTEventPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.nft_event_post);
  console.log(stderr);
};

const FTEventFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.ft_event_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.ft_event_file.command
  );
  console.log(stderr);
};

const FTEventPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.ft_event_post);
  console.log(stderr);
};

const contractDeploymentFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.contract_deployment_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.contract_deployment_file.command
  );
  console.log(stderr);
};

const contractDeploymentPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(
    predicateCommands.contract_deployment_post
  );
  console.log(stderr);
};

const contractCallFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.contract_call_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.contract_call_file.command
  );
  console.log(stderr);
};

const contractCallPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.contract_call_post);
  console.log(stderr);
};

const blockHeightFilePredicate = async (): Promise<any> => {
  fs.writeFileSync(predicateCommands.block_height_file.result_file, "");
  const { stdout, stderr } = await exec(
    predicateCommands.block_height_file.command
  );
  console.log(stderr);
};

const blockHeightPOSTPredicate = async (): Promise<any> => {
  const { stdout, stderr } = await exec(predicateCommands.block_height_post);
  console.log(stderr);
};
