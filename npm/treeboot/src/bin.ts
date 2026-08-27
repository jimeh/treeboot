#!/usr/bin/env node

import { runCli } from "./cli.js";

runCli(process.argv.slice(2)).then(
  (code) => {
    process.exitCode = code;
  },
  (error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`treeboot: ${message}\n`);
    process.exitCode = 1;
  },
);
