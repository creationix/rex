#!/bin/bash
bun build --watch src/main.ts --outdir dist --format esm --target browser --minify
