// Calculate the fibonacci numbers up to max (default 100)
// rex run examples/fibonacci.rex
// rex run examples/fibonacci.rex max=200
extern max: int | none
max = max or 100

// Declare an external function to print the results
extern "P" print(some) -> some

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

print(fibs)

// Functional: while comprehension
a = 1; b = 1
fibs2 = [ v = a; c = a + b; a = b; b = c; v while a <= max ]

print(fibs2)

// Verify both methods give the same result
when fibs == fibs2 do
  "fibs and fibs2 are the same"
else
  "fibs and fibs2 are different"
end
