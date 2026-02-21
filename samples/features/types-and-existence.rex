// Type predicates and existence-based logic
// rex samples/features/types-and-existence.rex

inputs = [42 "hello" [1 2] {x: 1} true undefined null]
tags = []

for i, value in inputs do
  when n = number(value) do
    tags.(i) = "number:" + n
  else when s = string(value) do
    tags.(i) = "string:" + s
  else when array(value) do
    tags.(i) = "array"
  else when object(value) do
    tags.(i) = "object"
  else when boolean(value) do
    tags.(i) = "boolean"
  else
    tags.(i) = "absent:" + not value
  end
end

filtered = [self != null and self in inputs]

{
  tags: tags
  filtered: filtered
}
