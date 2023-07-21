import {
  triggerAllPredicates,
  triggerAllFilePredicates,
  triggerAllPOSTPredicates,
} from "./stacks-predicates";
import { initDomainAPI, clearResultFiles } from "./utils/helper";
require("dotenv").config({ path: ".env" });

const type = process.argv[2];
if (!type) {
  initDomainAPI();
  triggerAllPredicates();
}

if (type === "file") {
  triggerAllFilePredicates();
}

if (type === "post") {
  triggerAllPOSTPredicates();
}

if (type === "clear-result-files") {
  clearResultFiles();
}