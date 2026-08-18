#!/usr/bin/env node
import process from "node:process";
import { CORE_DEV_PROXY_HOOKS } from "./core-dev-proxy.js";
import { reportStdioFailure, runStdio } from "./stdio-runner.js";

function option(name: string): string | undefined {
  const index = process.argv.indexOf(name);
  return index < 0 ? undefined : process.argv[index + 1];
}

function required(name: string): string {
  const value = option(name);
  if (!value) throw new Error(`missing required option ${name}`);
  return value;
}

if (process.argv[2] !== "stdio") {
  throw new Error("Core-dev runner accepts only the fixed stdio mode");
}
runStdio(
  {
    apiPath: required("--api-doc"),
    examples: [required("--example"), required("--example-secondary")],
  },
  CORE_DEV_PROXY_HOOKS,
).catch(reportStdioFailure);
