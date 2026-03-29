import { readFileSync } from 'fs';
import { encode } from './index.js';

const jsonPath = '/Users/tim/Code/routes-data/data/vercel-marketing-scraped-metadata.json';
const data = JSON.parse(readFileSync(jsonPath, 'utf8'));

// Warm up
encode(data);

// Profile this
console.profile('encode');
const t = performance.now();
const out = encode(data);
console.log(`encode: ${(performance.now() - t).toFixed(1)}ms, ${out.length} bytes`);
console.profileEnd('encode');
