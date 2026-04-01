// Calculate factorials up to max (default 10)
// rex examples/algorithms/factorial.rex
// rex -e 'max = 15' examples/algorithms/factorial.rex

max = max or 10

factorials = []
i = 0
n = 0
fact = 1
while n <= max do
  factorials.(i) = fact
  i += 1
  n += 1
  fact *= n
end

factorials
