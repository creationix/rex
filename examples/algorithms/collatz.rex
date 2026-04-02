// Generate Collatz sequence lengths up to max (default 100)
// rex run examples/algorithms/collatz.rex
// rex run examples/algorithms/collatz.rex max=200

extern max: int | none
max = max or 100

lengths: { *: int } = {}
n = 1
while n <= max do
  current = n
  steps = 0
  while current != 1 do
    when current % 2 == 0 do
      current = current / 2
    else
      current = 3 * current + 1
    end
    steps += 1
  end
  lengths.(n) = steps
  n += 1
end

[ lengths.(v) for v in 1 .. max ]
