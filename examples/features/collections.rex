// Collections and comprehensions
// rex examples/features/collections.rex

items = [1 2 3 4 5]
squares = [v * v for v in items]
evens = [v % 2 == 0 and v for v in items]

users: [{name: str, score: int}] = [
  {name: "Ada" score: 95}
  {name: "Ben" score: 72}
  {name: "Cia" score: 88}
]

scores-by-name = {(u.name): u.score for u in users}
honor-roll = [u.score >= 85 and u.name for u in users]

key = "Ada"
ada-score = scores-by-name.(key)

{
  items: items
  squares: squares
  evens: evens
  honor-roll: honor-roll
  ada-score: ada-score
}
