// Ranges and bitwise operators
// rex examples/features/ranges-and-bitwise.rex

ascending = [v for v in 1..5]
descending = [v for v in 5..1]

mask = 0x0
bit = 0b0001
for v in 1..4 do
  mask = mask | bit
  bit = bit * 2
end

{
  ascending: ascending
  descending: descending
  mask: mask
}
