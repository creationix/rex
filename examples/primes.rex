// Calculate all primes up to max (default 100)
// rex run examples/primes.rex
// rex run examples/primes.rex max=200

extern max: int | none
max = max or 100

// Sieve of Eratosthenes — mark composites in an object
composites: { *: bool } = {}
n = 2
while n * n <= max do
  unless composites.(n) do
    m = n * n
    while m <= max do
      composites.(m) = true
      m += n
    end
  end
  n += 1
end

// Collect primes with an array comprehension
// Note: uses `and none or n` pattern for existence-based filtering
[ composites.(n) and none or n for n in 2 .. max ]
