// Calculate all primes up to max (default 100)
// rex primes.rex
// rex -e 'max = 200' primes.rex
max = max or 100

// Sieve of Eratosthenes — mark composites in an object
composites = {}
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
[max - 1 ; do
  n = self + 1
  unless composites.(n) do n end
end]
