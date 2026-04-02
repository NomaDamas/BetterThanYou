#!/usr/bin/env node
import { runCli } from "../src/cli.js";

runCli().then(
  code => {
    process.exitCode = code;
  },
  error => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
);
