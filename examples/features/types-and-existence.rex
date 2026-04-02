// Type predicates and existence-based logic
// rex examples/features/types-and-existence.rex

inputs = [ 42 "hello" [ 1 2 ] { x: 1 } true none null ]
tags = []

for i, value in inputs do
  when n = isNumber(value) do
    tags.(i) = `number:${n}`
  else when s = isString(value) do
    tags.(i) = `string:${s}`
  else when isArray(value) do
    tags.(i) = "array"
  else when isObject(value) do
    tags.(i) = "object"
  else when isBoolean(value) do
    tags.(i) = "boolean"
  else
    tags.(i) = "absent"
  end
end

filtered = [ v != null and v for v in inputs ]

{
  tags: tags
  filtered: filtered
}
