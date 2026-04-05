// Calculate the fibonacci numbers up to max (default 100)
// rex run examples/fibonacci.rex
// rex run examples/fibonacci.rex max=200
extern max: int | none
max = max or 100

// Imperative: build with push
fibs = []
a = 1
b = 1
while a <= max do
  fibs.push(a)
  c = a + b
  a = b
  b = c
end
fibs

// Functional: while comprehension
a = 1; b = 1
[ v = a; c = a + b; a = b; b = c; v while a <= max ]
