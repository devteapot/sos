#!/usr/bin/env node
import process from "node:process";
import { runCli } from "./main.js";
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

if (process.argv[2] === "stdio") {
  runStdio({
    apiPath: required("--api-doc"),
    examples: [required("--example"), required("--example-secondary")],
  }).catch(reportStdioFailure);
} else {
  runCli().catch((error: Error) => {
    console.error(`sos_agent_failed error=${error.message}`);
    process.exitCode = 1;
  });
}
