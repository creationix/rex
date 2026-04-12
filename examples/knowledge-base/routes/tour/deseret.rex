/* Tour: Deseret Alphabet Translator
   Demonstrates http.fetch for proxying to external APIs
   and Unicode support with the Deseret script (U+10400-U+1044F). */
res.headers.content-type = "text/html; charset=utf-8"
layout = fs.read("routes/_layouts/page.html")
unless layout do
  status = 500
  return "layout not found"
end

/* Handle translation form submission */
input-text = query.text
direction = query.dir or "to-deseret"
translated = none
error-msg = none

when input-text do
  api-body = when direction == "to-english" do
    json.stringify({deseret: input-text})
  else
    json.stringify({english: input-text})
  end

  result = http.fetch("http://2deseret.com/json/translation", {
    method: "POST"
    headers: {"content-type": "application/json"}
    body: api-body
  })

  when result.status == 200 do
    data = json.parse(result.body)
    translated = when direction == "to-english" do
      data.english
    else
      data.deseret
    end
  else
    error-msg = `API error: status ${result.status}`
  end
end

/* Build page */
result-html = when translated do
  html`<div class="card" style="margin-top:1rem">
    <h3>${when direction == "to-english" do "English" else "Deseret (𐐔𐐯𐑅𐐨𐑉𐐯𐐻)" end}</h3>
    <p style="font-size:1.5rem;line-height:1.8">${translated}</p>
  </div>`
else when error-msg do
  html`<div class="card" style="margin-top:1rem;border-color:red">
    <p>${error-msg}</p>
  </div>`
else
  ""
end

sample-english = "The Deseret Alphabet was created in the 1850s."

body = html`<h1>Deseret Alphabet Translator</h1>
<p class="source-link"><a href="/tour/experience">&larr; DX Report</a></p>

<p>The <a href="https://en.wikipedia.org/wiki/Deseret_alphabet">Deseret Alphabet</a>
(𐐔𐐯𐑅𐐨𐑉𐐯𐐻) is a phonemic English-language alphabet devised in the 1850s by the
University of Deseret (now the University of Utah). This page demonstrates
<code>http.fetch</code> for proxying to external APIs and Unicode support
with characters outside the Basic Multilingual Plane.</p>

<h2>Try It</h2>
<form method="GET" action="/tour/deseret" style="display:flex;flex-direction:column;gap:0.75rem">
  <div style="display:flex;gap:1rem;align-items:center">
    <label><input type="radio" name="dir" value="to-deseret" ${when direction != "to-english" do "checked" else "" end}> English &rarr; Deseret</label>
    <label><input type="radio" name="dir" value="to-english" ${when direction == "to-english" do "checked" else "" end}> Deseret &rarr; English</label>
  </div>
  <textarea name="text" rows="3" style="width:100%;font-size:1.1rem;padding:0.5rem;background:var(--card-bg);color:var(--fg);border:1px solid var(--border);border-radius:4px">${input-text or sample-english}</textarea>
  <button type="submit" style="align-self:flex-start;padding:0.5rem 1.5rem;background:var(--accent);color:white;border:none;border-radius:4px;cursor:pointer;font-size:1rem">Translate</button>
</form>

${html.raw(result-html)}

<h2>How It Works</h2>
<div class="card">
<p>This page uses <code>http.fetch</code> to POST JSON to the
<a href="https://www.2deseret.com/api">2deseret.com API</a>:</p>
<pre>result = http.fetch("http://2deseret.com/json/translation", {
  method: "POST"
  headers: {"content-type": "application/json"}
  body: json.stringify({english: input-text})
})</pre>
<p>The response is a JSON object with the translated text. Deseret characters
are Unicode codepoints in the Supplementary Multilingual Plane (U+10400&ndash;U+1044F),
requiring 4-byte UTF-8 sequences &mdash; a good test of the server's Unicode handling.</p>
</div>

<h2>The Deseret Alphabet</h2>
<p>All 38 letters with their uppercase (𐐀-𐐥) and lowercase (𐐨-𐑍) forms:</p>
<table style="width:100%;border-collapse:collapse;font-size:1rem">
<tr style="border-bottom:1px solid var(--border)">
  <th style="padding:0.4rem">Upper</th><th style="padding:0.4rem">Lower</th>
  <th style="text-align:left;padding:0.4rem">Name</th>
  <th style="text-align:left;padding:0.4rem">Sound</th>
</tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐀</td><td style="font-size:1.4rem;text-align:center">𐐨</td><td>Long I</td><td><em>ice</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐁</td><td style="font-size:1.4rem;text-align:center">𐐩</td><td>Long E</td><td><em>eat</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐂</td><td style="font-size:1.4rem;text-align:center">𐐪</td><td>Long Ah</td><td><em>art</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐃</td><td style="font-size:1.4rem;text-align:center">𐐫</td><td>Long Aw</td><td><em>ought</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐄</td><td style="font-size:1.4rem;text-align:center">𐐬</td><td>Long O</td><td><em>old</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐅</td><td style="font-size:1.4rem;text-align:center">𐐭</td><td>Long Oo</td><td><em>ooze</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐆</td><td style="font-size:1.4rem;text-align:center">𐐮</td><td>Short I</td><td><em>it</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐇</td><td style="font-size:1.4rem;text-align:center">𐐯</td><td>Short E</td><td><em>egg</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐈</td><td style="font-size:1.4rem;text-align:center">𐐰</td><td>Short Ah</td><td><em>at</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐉</td><td style="font-size:1.4rem;text-align:center">𐐱</td><td>Short Aw</td><td><em>on</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐊</td><td style="font-size:1.4rem;text-align:center">𐐲</td><td>Short O</td><td><em>up</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐋</td><td style="font-size:1.4rem;text-align:center">𐐳</td><td>Short Oo</td><td><em>book</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐌</td><td style="font-size:1.4rem;text-align:center">𐐴</td><td>Ay</td><td><em>ale</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐍</td><td style="font-size:1.4rem;text-align:center">𐐵</td><td>Ow</td><td><em>out</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐎</td><td style="font-size:1.4rem;text-align:center">𐐶</td><td>Wu</td><td><em>wet</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐏</td><td style="font-size:1.4rem;text-align:center">𐐷</td><td>Yee</td><td><em>yet</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐐</td><td style="font-size:1.4rem;text-align:center">𐐸</td><td>H</td><td><em>hay</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐑</td><td style="font-size:1.4rem;text-align:center">𐐹</td><td>Pee</td><td><em>pet</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐒</td><td style="font-size:1.4rem;text-align:center">𐐺</td><td>Bee</td><td><em>bet</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐓</td><td style="font-size:1.4rem;text-align:center">𐐻</td><td>Tee</td><td><em>ten</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐔</td><td style="font-size:1.4rem;text-align:center">𐐼</td><td>Dee</td><td><em>den</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐕</td><td style="font-size:1.4rem;text-align:center">𐐽</td><td>Chee</td><td><em>check</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐖</td><td style="font-size:1.4rem;text-align:center">𐐾</td><td>Jee</td><td><em>jet</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐗</td><td style="font-size:1.4rem;text-align:center">𐐿</td><td>Kay</td><td><em>kit</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐘</td><td style="font-size:1.4rem;text-align:center">𐑀</td><td>Gay</td><td><em>get</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐙</td><td style="font-size:1.4rem;text-align:center">𐑁</td><td>Ef</td><td><em>fit</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐚</td><td style="font-size:1.4rem;text-align:center">𐑂</td><td>Vee</td><td><em>vet</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐛</td><td style="font-size:1.4rem;text-align:center">𐑃</td><td>Eth</td><td><em>then</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐜</td><td style="font-size:1.4rem;text-align:center">𐑄</td><td>Thee</td><td><em>thin</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐝</td><td style="font-size:1.4rem;text-align:center">𐑅</td><td>Es</td><td><em>sit</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐞</td><td style="font-size:1.4rem;text-align:center">𐑆</td><td>Zee</td><td><em>zoo</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐟</td><td style="font-size:1.4rem;text-align:center">𐑇</td><td>Esh</td><td><em>she</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐠</td><td style="font-size:1.4rem;text-align:center">𐑈</td><td>Zhee</td><td><em>vision</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐡</td><td style="font-size:1.4rem;text-align:center">𐑉</td><td>Er</td><td><em>red</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐢</td><td style="font-size:1.4rem;text-align:center">𐑊</td><td>El</td><td><em>let</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐣</td><td style="font-size:1.4rem;text-align:center">𐑋</td><td>Em</td><td><em>met</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐤</td><td style="font-size:1.4rem;text-align:center">𐑌</td><td>En</td><td><em>net</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐥</td><td style="font-size:1.4rem;text-align:center">𐑍</td><td>Eng</td><td><em>sing</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐦</td><td style="font-size:1.4rem;text-align:center">𐑎</td><td>Oi</td><td><em>oil</em></td></tr>
<tr><td style="font-size:1.4rem;text-align:center">𐐧</td><td style="font-size:1.4rem;text-align:center">𐑏</td><td>Ew</td><td><em>cute</em></td></tr>
</table>`

template.render(layout, {
  title: "Deseret Translator"
  body: body
  footer: "<a href='/tour/experience'>&larr; DX Report</a> &middot; <a href='/'>Home</a>"
})
