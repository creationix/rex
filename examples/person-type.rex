type Person = { name: str color: int }

db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}

first-name = db.bob.name
tim-color = db.tim.color

// Dynamic nav with literal string resolves exact type
dynamic-tim = db.("tim")

// Conditional scope merging: both branches assign, original dropped
extern x: some | none
key = "tim"
when x do
  key = "bob"
else
  key = "cat"
end
// key: "bob" | "cat" (not "tim")

// Else-when chain: no final else, so original is kept
extern y: int | none
extern z: int | none
tag = "default"
when y do
  tag = "yes"
else when z do
  tag = "maybe"
end
// tag: "default" | "yes" | "maybe"

// While with certain condition replaces outer type
extern d: some
label = "old"
while d do
  label = "new"
  break
end
// label: "new" (d is never none, body always runs)

// Dynamic nav with conditional key
res = db.(key)

[ first-name tim-color dynamic-tim tag label res ]
