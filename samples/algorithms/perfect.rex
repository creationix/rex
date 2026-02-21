// Find perfect numbers up to max (default 1000)
// rex samples/algorithms/perfect.rex
// rex -e 'max = 2000' samples/algorithms/perfect.rex

max = max or 1000

perfects = []
n = 2
i = 0
while n <= max do
  sum = 0
  i_div = 1
  while i_div * i_div <= n do
    when n % i_div == 0 do
      when i_div != n do
        sum += i_div
      end
      when i_div * i_div != n and n / i_div != n do
        sum += n / i_div
      end
    end
    i_div += 1
  end

  when sum == n do
    perfects.(i) = n
    i += 1
  end

  n += 1
end

perfects
