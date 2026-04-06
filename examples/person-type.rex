type Person = { name: str color: int }

db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}

first-name = db.bob.name

tim-color = db.tim.color

dynamic-key = "tim"
dynamic-tim = db.(dynamic-key)

[first-name tim-color dynamic-tim]