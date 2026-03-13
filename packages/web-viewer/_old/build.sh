#!/bin/bash
bun build --watch src/main.ts src/decode-worker.ts --outdir dist --format esm --target browser --minify
