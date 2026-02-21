// Calculate triangular numbers up to max (default 100)
// rex samples/algorithms/triangular.rex
// rex -e 'max = 200' samples/algorithms/triangular.rex

max = max or 100

triangles = []
i = 0
n = 1
sum = 0
while sum <= max do
  sum += n
  triangles.(i) = sum
  i += 1
  n += 1
end

triangles
