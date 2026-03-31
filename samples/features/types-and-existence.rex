// Type predicates and existence-based logic
// rex samples/features/types-and-existence.rex

inputs = [42 "hello" [1 2] {x: 1} true none null]
tags = []

for i, value in inputs do
  when n = number(value) do
    tags.(i) = `number:${n}`
  else when s = string(value) do
    tags.(i) = `string:${s}`
  else when array(value) do
    tags.(i) = "array"
  else when object(value) do
    tags.(i) = "object"
  else when boolean(value) do
    tags.(i) = "boolean"
  else
    tags.(i) = "absent"
  end
end

filtered = [v != null and v for v in inputs]

{
  tags: tags
  filtered: filtered
}
