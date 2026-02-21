// Collections and comprehensions
// rex samples/features/collections.rex

items = [1 2 3 4 5]
squares = [self * self for in items]
evens = [self % 2 == 0 and self for in items]

users = [
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
