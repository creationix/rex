import { readFileSync } from 'fs';
import { encode } from './index.js';

const jsonPath = '/Users/tim/Code/routes-data/data/vercel-marketing-scraped-metadata.json';
const data = JSON.parse(readFileSync(jsonPath, 'utf8'));

// Warm up
encode(data);
encode(data);

// Run multiple times so samply can get good samples
for (let i = 0; i < 5; i++) {
  encode(data);
}
console.log('done');
