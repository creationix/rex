// Ranges and bitwise operators
// rex samples/features/ranges-and-bitwise.rex

ascending = [self in 1..5]
descending = [self in 5..1]

mask = 0x0
bit = 0b0001
for in 1..4 do
  mask = mask | bit
  bit = bit * 2
end

{
  ascending: ascending
  descending: descending
  mask: mask
}
