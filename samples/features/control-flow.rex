// Control flow: when/unless, for, while, break/continue
// rex samples/features/control-flow.rex

value = value or 7

when value > 10 do
  status = "high"
else when value > 0 do
  status = "low"
else
  status = "zero"
end

sum = 0
for i in 1..5 do
  when i == 4 do
    continue
  else
    sum += i
  end
end

countdown = 3
while countdown > 0 do
  countdown -= 1
end

{status: status sum: sum countdown: countdown}
