// Gradebook case study — analyze student scores across subjects
// rex samples/case-studies/gradebook.rex

pass-threshold = pass-threshold or 60
max-grade = max-grade or 100
bonus = 10

students = [
  {name: "Alice" scores: {math: 92 science: 88 english: 95 history: 78}}
  {name: "Bob" scores: {math: 45 science: 62 english: 58 history: 71}}
  {name: "Carol" scores: {math: 73 science: 81 english: 69 history: 84}}
  {name: "Dave" scores: {math: 88 science: 55 english: 91 history: 63}}
  {name: "Eve" scores: {math: 100 science: 97 english: 99 history: 96}}
]

subjects = [self of students.0.scores]
results = []

for i, student in students do
  total = 0
  count = 0
  grades = {}

  for subj, score in student.scores do
    curved = score + bonus
    when curved > max-grade do
      curved = max-grade
    end

    grades.(subj) = {raw: score curved: curved}
    total += curved
    count += 1
  end

  avg = total / count
  results.(i) = {name: student.name average: avg grades: grades}
end

honors = [r.average >= 85 and r.name for r in results]

at-risk = []
at-risk-n = 0
for student in results do
  failed = false
  for subj, detail in student.grades do
    unless detail.curved >= pass-threshold do
      failed = true
    end
  end
  when failed == true do
    at-risk.(at-risk-n) = student.name
    at-risk-n += 1
  end
end

subject-averages = {}
for subj in subjects do
  sum = 0
  for student in results do
    sum += student.grades.(subj).curved
  end
  subject-averages.(subj) = sum / results.size
end

class-total = 0
for in results do
  class-total += self.average
end
class-avg = class-total / results.size

{
  honors: honors
  at-risk: at-risk
  subject-averages: subject-averages
  class-average: class-avg
  student-count: results.size
}
