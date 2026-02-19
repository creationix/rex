// Calculate the fibonacci numbers up to max (default 100)
// rex fibonacci.rex
// rex -e 'max = 200' fibonacci.rex
max = max or 100

// Build fibonacci sequence using index keys
fibs = {}
i = 0
a = 1
b = 1
while a <= max do
  i += 1
  fibs.(i) = a
  c = a + b
  a = b
  b = c
end

// Collect object values into an array
[self in fibs]
