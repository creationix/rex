import { readFileSync } from 'fs';
import { encode, compile } from './index.js';

const jsonPath = '/Users/tim/Code/routes-data/data/vercel-marketing-scraped-metadata.json';

console.log('Loading JSON...');
const jsonStr = readFileSync(jsonPath, 'utf8');
console.log(`JSON string: ${(jsonStr.length / 1048576).toFixed(1)} MB`);

const t0 = performance.now();
const data = JSON.parse(jsonStr);
console.log(`JSON.parse: ${(performance.now() - t0).toFixed(1)}ms\n`);

const runs = 5;

function bench(name, fn) {
  fn(); // warm up
  const times = [];
  let output;
  for (let i = 0; i < runs; i++) {
    const t = performance.now();
    output = fn();
    times.push(performance.now() - t);
  }
  times.sort((a, b) => a - b);
  const median = times[Math.floor(runs / 2)];
  console.log(`${name}:`);
  console.log(`  output: ${(output.length / 1048576).toFixed(2)} MB (${(output.length / jsonStr.length * 100).toFixed(1)}%)`);
  console.log(`  median: ${median.toFixed(1)}ms`);
  console.log(`  all:    [${times.map(t => t.toFixed(1)).join(', ')}]\n`);
}

bench('encode(value)', () => encode(data));
bench('compile(source)', () => compile(jsonStr));
