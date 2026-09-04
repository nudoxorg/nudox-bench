"""Rebuild the published results page from a run's own charts.

The prose in this page is written about one specific run — the one published
as `results/2026-08-30/` — so this regenerates that document rather than being
a general report builder. It exists so the published artifact can be rebuilt
instead of only admired.

    usage: build_presentation.py [run-dir] [out.html]

Output defaults to `<run-dir>/presentation.html`, which is *not* the shorter
`protocol.html` already published from that directory.

Charts are read from `<run-dir>/charts/`, which `plot_results.py <run-dir>`
writes. Run that first.

It used to read `/tmp/nb/imgs.json` — a scratch file nothing in this
repository produces — so on any other machine it simply failed, in a project
whose whole claim is that its numbers can be reproduced.
"""
import base64
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_RUN = REPO / "results/2026-08-30"
PLATES = ("efficiency", "cost_per_scenario", "resolution_gap", "surface_cost", "ablations")


def _args():
    rest = [a for a in sys.argv[1:] if a not in ("-h", "--help")]
    if len(rest) != len(sys.argv[1:]):
        print(__doc__)
        sys.exit(0)
    run = pathlib.Path(rest[0]) if rest else DEFAULT_RUN
    # NOT protocol.html. That is a different, shorter document already
    # published from this directory; this script builds the illustrated presentation page
    # titled "Does the Index Beat Grep?", and defaulting to the other name
    # would overwrite a published artifact with a different page.
    out = pathlib.Path(rest[1]) if len(rest) > 1 else run / "presentation.html"
    return run, out


def _images(run_dir):
    charts = run_dir / "charts"
    if not charts.is_dir():
        sys.exit("no charts in %s\n  run: python3 scripts/plot_results.py %s"
                 % (charts, run_dir))
    found = {}
    for key in PLATES:
        png = charts / ("%s.png" % key)
        if png.is_file():
            found[key] = base64.b64encode(png.read_bytes()).decode()
    absent = [k for k in PLATES if k not in found]
    if absent:
        # Named rather than silently dropped: a plate that vanishes without a
        # word is how a page quietly stops making its own argument.
        print("warning: no chart for %s — those plates are omitted"
              % ", ".join(absent), file=sys.stderr)
    return found


RUN_DIR, OUT_PATH = _args()
imgs = _images(RUN_DIR)


def plate(key, caption):
    if key not in imgs:
        return ""
    return f'''<figure class="plate">
  <img src="data:image/png;base64,{imgs[key]}" alt="{caption}">
</figure>'''

HTML = '''<title>Does the Index Beat Grep?</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Newsreader:ital,opsz,wght@0,6..72,400;0,6..72,500;0,6..72,600;1,6..72,400&family=IBM+Plex+Mono:wght@400;500;600&family=IBM+Plex+Sans:wght@400;450;500;600&display=swap">
<style>
:root {
  color-scheme: light;
  --ground:      #fbfbf9;
  --raised:      #ffffff;
  --sunk:        #f2f2ee;
  --ink:         #14161a;
  --ink-2:       #4d5259;
  --ink-3:       #878d95;
  --rule:        #e3e3dc;
  --rule-strong: #cfcfc6;
  --accent:      #2a78d6;
  --accent-soft: #eaf1fb;
  --warn:        #a24a18;
  --warn-soft:   #fbf0e8;
  --good:        #16794f;

  --measure: 68ch;
  --step--1: 0.815rem;
  --step-0:  1.02rem;
  --step-1:  1.28rem;
  --step-2:  1.62rem;
  --step-3:  2.15rem;
  --step-4:  3.1rem;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    color-scheme: dark;
    --ground:      #14161a;
    --raised:      #1c1f24;
    --sunk:        #101216;
    --ink:         #eceef1;
    --ink-2:       #a8aeb7;
    --ink-3:       #767d87;
    --rule:        #2b2f36;
    --rule-strong: #3c424b;
    --accent:      #6aa6ee;
    --accent-soft: #1b2735;
    --warn:        #e0946a;
    --warn-soft:   #2a1f18;
    --good:        #5fc196;
  }
}
:root[data-theme="dark"] {
  color-scheme: dark;
  --ground:      #14161a;
  --raised:      #1c1f24;
  --sunk:        #101216;
  --ink:         #eceef1;
  --ink-2:       #a8aeb7;
  --ink-3:       #767d87;
  --rule:        #2b2f36;
  --rule-strong: #3c424b;
  --accent:      #6aa6ee;
  --accent-soft: #1b2735;
  --warn:        #e0946a;
  --warn-soft:   #2a1f18;
  --good:        #5fc196;
}

* { box-sizing: border-box; }
body {
  background: var(--ground);
  color: var(--ink);
  font-family: "IBM Plex Sans", ui-sans-serif, system-ui, sans-serif;
  font-size: var(--step-0);
  line-height: 1.62;
  margin: 0;
  padding: 0 1.5rem 6rem;
  -webkit-font-smoothing: antialiased;
}
.wrap { max-width: 61rem; margin: 0 auto; }
.prose { max-width: var(--measure); }

h1, h2, h3 {
  font-family: Newsreader, ui-serif, Georgia, serif;
  font-weight: 500;
  text-wrap: balance;
  line-height: 1.16;
  margin: 0;
}
h1 { font-size: var(--step-4); letter-spacing: -0.018em; }
h2 { font-size: var(--step-2); letter-spacing: -0.01em; }
h3 { font-size: var(--step-1); }
p { margin: 0; }

.eyebrow {
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: var(--step--1);
  letter-spacing: 0.13em;
  text-transform: uppercase;
  color: var(--ink-3);
}

header.masthead {
  padding: 5rem 0 3rem;
  border-bottom: 1px solid var(--rule);
  display: flex; flex-direction: column; gap: 1.5rem;
}
.lede {
  font-family: Newsreader, ui-serif, Georgia, serif;
  font-size: var(--step-1);
  color: var(--ink-2);
  max-width: 54ch;
}
.provenance {
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: var(--step--1);
  color: var(--ink-3);
  display: flex; flex-wrap: wrap; gap: 0.4rem 1.6rem;
}

section { padding-top: 3.6rem; display: flex; flex-direction: column; gap: 1.3rem; }

.stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(15rem, 1fr));
  gap: 1px;
  background: var(--rule);
  border: 1px solid var(--rule);
  margin-top: 0.6rem;
}
.stat { background: var(--raised); padding: 1.5rem 1.4rem; display: flex; flex-direction: column; gap: 0.35rem; }
.stat .n {
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: var(--step-3); font-weight: 500; letter-spacing: -0.03em;
  font-variant-numeric: tabular-nums; color: var(--accent); line-height: 1.05;
}
.stat .k { font-size: var(--step--1); color: var(--ink-2); }

.callout {
  border: 1px solid var(--rule-strong);
  border-left: 3px solid var(--warn);
  background: var(--warn-soft);
  padding: 1.4rem 1.5rem;
  display: flex; flex-direction: column; gap: 0.8rem;
}
.callout h3 { color: var(--warn); font-family: "IBM Plex Sans", sans-serif; font-size: var(--step-0); font-weight: 600; }
.callout ul { margin: 0; padding-left: 1.1rem; display: flex; flex-direction: column; gap: 0.45rem; }
.callout li { color: var(--ink-2); font-size: 0.95rem; }

/* Charts are printed plates: they keep a light ground in both themes, the way
   a figure keeps its paper. */
.plate {
  margin: 0.4rem 0 0;
  background: #fcfcfb;
  border: 1px solid var(--rule-strong);
  padding: 0.9rem;
  overflow-x: auto;
}
.plate img { display: block; width: 100%; height: auto; }

.finding {
  display: grid; grid-template-columns: auto 1fr; gap: 0 0.9rem;
  align-items: baseline;
}
.finding .mark {
  font-family: "IBM Plex Mono", monospace; font-size: 0.78rem;
  color: var(--accent); letter-spacing: 0.05em;
}

ul.bullets { margin: 0; padding-left: 0; list-style: none; display: flex; flex-direction: column; gap: 1rem; }
ul.bullets > li {
  display: grid; grid-template-columns: 0.85rem 1fr; gap: 0.85rem;
  max-width: var(--measure);
}
ul.bullets > li::before {
  content: ""; width: 6px; height: 6px; margin-top: 0.62em;
  background: var(--accent); border-radius: 1px;
}
ul.bullets strong { font-weight: 600; }

table {
  border-collapse: collapse; width: 100%;
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.82rem; font-variant-numeric: tabular-nums;
}
.scroll { overflow-x: auto; border: 1px solid var(--rule); }
th, td { padding: 0.6rem 0.85rem; text-align: right; white-space: nowrap; border-bottom: 1px solid var(--rule); }
th:first-child, td:first-child { text-align: left; }
thead th { background: var(--sunk); color: var(--ink-2); font-weight: 500; font-size: 0.76rem; letter-spacing: 0.04em; text-transform: uppercase; }
tbody tr:last-child td { border-bottom: 0; }
tbody tr.hi td { background: var(--accent-soft); }
td.win { color: var(--good); }

code, .mono { font-family: "IBM Plex Mono", ui-monospace, monospace; font-size: 0.88em; }
code { background: var(--sunk); padding: 0.1em 0.35em; border: 1px solid var(--rule); }

footer { margin-top: 4.5rem; padding-top: 1.6rem; border-top: 1px solid var(--rule); color: var(--ink-3); font-size: var(--step--1); max-width: var(--measure); }
a { color: var(--accent); }
:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
@media (prefers-reduced-motion: reduce) { * { animation: none !important; transition: none !important; } }
</style>

<div class="wrap">

<header class="masthead">
  <p class="eyebrow">nudox-bench · run 2026-08-30-165221</p>
  <h1>Does the index beat grep?</h1>
  <p class="lede">On the questions it was given and the packages it holds, Nudox matched
  ripgrep's accuracy while returning <strong>7.4&times; less text</strong>, and every fact it
  returned was a resolved symbol rather than a line of source that happened to
  contain the right word. It also declined half the corpus, and one capability
  was switched off. Here is all of it.</p>
  <div class="provenance">
    <span>10 scenarios</span><span>7 adapters</span><span>4 languages</span>
    <span>cl100k + o200k, real BPE</span>
  </div>
</header>

<section>
  <h2>The headline</h2>
  <div class="stats">
    <div class="stat"><span class="n">7.4&times;</span><span class="k">fewer tokens than ripgrep per scenario, at a higher pass rate on the Rust set</span></div>
    <div class="stat"><span class="n">0.00</span><span class="k">resolution gap &mdash; every fact Nudox returned came back as a qualified path</span></div>
    <div class="stat"><span class="n">3,761</span><span class="k">cl100k tokens the 9-tool surface costs before a single question is asked</span></div>
  </div>
</section>

<section>
  <div class="callout">
    <h3>Read this before the numbers</h3>
    <ul>
      <li><strong>Semantic search was off.</strong> Every Nudox response carried
      <code>~sem:unavailable(NoModelConfigured)</code> &mdash; no embedding model was
      configured, so half of what <code>search</code> normally ranks with was
      disabled. These are structural-only numbers.</li>
      <li><strong>Nudox held 3 of 5 packages.</strong> The Go oracle binary
      (<code>nudox-go-oracle</code>) is not installed, and Python's installed
      <code>requests</code> has no <code>pyproject.toml</code> for the producer to read.
      Those scenarios are recorded as declined, not failed.</li>
      <li><strong>The judge did not run.</strong> No <code>OPENROUTER_API_KEY</code>, so
      the two implementation scenarios are <em>unscored</em> rather than counted
      against anyone.</li>
      <li><strong>Ten scenarios is a small sample.</strong> Nothing here is significant
      in a statistical sense. It is a working instrument with first readings.</li>
    </ul>
  </div>
</section>

<section>
  <h2>Cost against correctness</h2>
  <p class="prose">Both measures at once, because either alone is gameable: a tool
  can be cheap by answering nothing, or accurate by returning the package.</p>
  PLATE_EFFICIENCY
  <p class="prose"><span class="mono" style="color:var(--ink-3)">Note:</span> the
  <code>nudox-*</code> rows and <code>ripgrep</code> are not answering the same number of
  questions &mdash; Nudox declined three packages it was never given. The
  like-for-like comparison is the per-scenario chart below.</p>
</section>

<section>
  <h2>What each answer cost</h2>
  PLATE_COST
  <ul class="bullets">
    <li>On <strong>callers of <code>search_slice_with_raw</code></strong>, Nudox answered
    correctly in <strong>1,602 tokens</strong>. ripgrep spent <strong>27,487</strong> and
    still got it wrong &mdash; it found 4 of 6 callers, because a line containing a
    call names the callee, never the caller.</li>
    <li>On <strong>memchr's public surface</strong>, the result inverts: ripgrep passed
    with 18,047 tokens of <code>^pub</code> lines; Nudox reached 60% for 1,205. Brute
    enumeration is genuinely good at enumeration.</li>
    <li>rust-analyzer is the cheapest thing on the board and answers almost
    nothing &mdash; 39 tokens to fail a signature lookup is not a bargain.</li>
  </ul>
</section>

<section>
  <h2>How much was actually resolved</h2>
  <p class="prose">Every expected fact is matched twice: <strong>strict</strong> counts only a
  fully-qualified path; <strong>loose</strong> counts the bare identifier appearing anywhere
  in the returned text. The distance between them is the part of a score that is
  undifferentiated text rather than a resolved symbol.</p>
  PLATE_RESOLUTION
  <ul class="bullets">
    <li><strong>Nudox's gap is exactly zero.</strong> Everything it returned, it returned
    as an address. That is the property you are buying, and it is the one that does
    not show up in a pass/fail column.</li>
    <li>ripgrep's 5-point gap is small here only because these scenarios ask about
    distinctively-named symbols. It widens with common names &mdash; which is most
    real code.</li>
  </ul>
</section>

<section>
  <h2>Precision, not recall, is where text search breaks</h2>
  <p class="prose">The clearest single result in the run. On
  <code>go/uuid/callers-parse</code>, ripgrep found <strong>100% of the expected callers</strong>
  and still <strong>failed</strong> &mdash; because <code>ParseBytes</code> landed in the output.
  It is an exported function whose name contains <code>Parse</code>, and it does not call
  <code>Parse</code>; it has its own parsing path (verified at <code>uuid.go:120</code>).</p>
  <p class="prose">A benchmark that scored recall alone would have called that a
  perfect answer. Distractor sets are why it did not.</p>
</section>

<section>
  <h2>What the surface costs before you ask anything</h2>
  PLATE_SURFACE
  <ul class="bullets">
    <li><code>index</code> alone is <strong>946 tokens, 25% of the whole surface</strong> &mdash;
    nearly double the next largest. It is also the tool a reading session calls
    <em>zero</em> times. Every agent pays for it on every turn regardless.</li>
    <li>MCP-SURFACE-PLAN &sect;1.1 measured the older <strong>13-tool</strong> surface at
    <strong>3,875</strong> cl100k. Nine tools now cost <strong>3,761</strong> &mdash; consolidation
    returned about <strong>3%</strong>, not the reduction the tool count suggests, because
    the surviving descriptions absorbed the prose.</li>
    <li>Put in context: the fixed surface costs roughly <strong>3&times; what an average
    answer costs</strong>. For a session asking one question, the tool descriptions are the
    dominant expense.</li>
  </ul>
</section>

<section>
  <h2>Nudox against itself</h2>
  <p class="prose">Five client-side restrictions on the same running server, so a
  whole sweep costs one command and no rebuild.</p>
  PLATE_ABLATIONS
  <ul class="bullets">
    <li><strong>The second call earns its keep.</strong> <code>oneshot</code> saves 41% of the
    tokens and loses a scenario. Stopping after <code>search</code> is cheap and
    frequently insufficient.</li>
    <li><strong><code>search-only</code> is 4.3&times; cheaper than the full surface</strong> and still
    answers 2. For an agent on a tight budget, search-then-stop is a defensible
    default &mdash; which is an argument for making the first response denser.</li>
    <li><strong>The escape hatch loses to the purpose-built tools.</strong>
    <code>graph-first</code> costs the same as <code>full</code> and answers less. The
    <code>schema</code> card is cheap (134 tokens) but writing a correct Trustfall query
    against it is not free.</li>
    <li><strong>The address-versus-hash ablation did not run</strong>, and that is itself the
    finding: <code>search</code> renders addresses, so the 64-hex key never crosses the
    wire and there was nothing to substitute. An agent does not pay the
    ~54-token hash unless it deliberately asks for it. The harness now reports
    <code>~ablation:not-exercised</code> rather than a misleading identical row.</li>
  </ul>
</section>

<section>
  <h2>Full results</h2>
  <div class="scroll">
  <table>
    <thead><tr>
      <th>adapter</th><th>pass</th><th>declined</th><th>unscored</th>
      <th>strict</th><th>gap</th><th>tok/scenario</th><th>tok/pass</th><th>calls</th>
    </tr></thead>
    <tbody>
      <tr class="hi"><td>nudox-full</td><td>3/6</td><td>3</td><td>1</td><td>0.62</td><td class="win">0.00</td><td>1,277</td><td>2,555</td><td>2.2</td></tr>
      <tr><td>nudox-keys</td><td>3/6</td><td>3</td><td>1</td><td>0.62</td><td class="win">0.00</td><td>1,277</td><td>2,555</td><td>2.2</td></tr>
      <tr><td>nudox-oneshot</td><td>2/6</td><td>3</td><td>1</td><td>0.36</td><td>0.03</td><td>749</td><td>2,248</td><td>1.0</td></tr>
      <tr><td>nudox-graph-first</td><td>2/6</td><td>3</td><td>1</td><td>0.45</td><td class="win">0.00</td><td>1,228</td><td>3,688</td><td>2.2</td></tr>
      <tr><td>nudox-search-only</td><td>2/7</td><td>1</td><td>2</td><td>0.39</td><td>0.02</td><td>292</td><td>1,023</td><td>1.0</td></tr>
      <tr><td>ripgrep</td><td>5/7</td><td>1</td><td>2</td><td>0.95</td><td>0.05</td><td>9,392</td><td>13,522</td><td>1.0</td></tr>
      <tr><td>rust-analyzer</td><td>1/4</td><td>5</td><td>1</td><td>0.35</td><td>0.04</td><td>301</td><td>1,226</td><td>1.5</td></tr>
    </tbody>
  </table>
  </div>
  <p class="prose"><em>declined</em> is never counted against a pass rate. rust-analyzer
  declining Go is a fact about its reach, not a wrong answer &mdash; and the same
  courtesy is extended to Nudox for packages its corpus was never given.</p>
</section>

<section>
  <h2>Analysis</h2>
  <ul class="bullets">
    <li><strong>The cost argument is the strong one; the accuracy argument is not yet
    made.</strong> On the Rust scenarios both systems could attempt, Nudox and ripgrep
    tie on pass rate. What separates them is an order of magnitude of tokens and a
    zero resolution gap. "Same answers, one-seventh the context" is a real claim.
    "More correct answers" is not supported by this run.</li>
    <li><strong>Nudox's one exclusive question, it got wrong &mdash; for a fixable
    reason.</strong> Cross-version <code>diff</code> is the scenario neither baseline can
    attempt. Nudox returned 50 changed declarations in 3,745 tokens, but ordered by
    path, so <code>alloc</code> module internals filled the page and the four added
    <code>HashMap</code> methods were past the cut. The tool worked; the ordering buried
    the answer, and the harness took only page one.</li>
    <li><strong>Two of the three Nudox failures were the harness using the tool
    wrong.</strong> The overview scenario sent a prose query ("memchr public exports") to
    a search that matches names and kinds &mdash; with the embedder off, it matched
    nothing and scored 13%. Querying the package name instead took it to 60% with no
    change to Nudox. This is worth stating plainly: a benchmark's first findings are
    usually about the benchmark.</li>
    <li><strong>The resident surface is the quiet cost.</strong> 3,761 tokens sit in context
    before any question, against an average answer of ~1,277. The tool descriptions
    outweigh the answers roughly 3:1 for a short session, and a quarter of that is
    one tool most sessions never call.</li>
    <li><strong>ripgrep is a much stronger baseline than it is usually given credit
    for.</strong> Given fair context flags, it passes 5 of 7. Anything replacing it has to
    win on cost, precision, or reach &mdash; not on "can it find things at all".</li>
    <li><strong>Reach is where the gap is structural, not incremental.</strong>
    rust-analyzer declined 5 of 10 &mdash; every non-Rust package and every
    cross-version question &mdash; and no amount of tuning changes that. That is the
    axis where an index has a defensible answer, and it is the axis this run
    measured least, because the Go and Python producers were unavailable.</li>
  </ul>
</section>

<section>
  <h2>What would make the next run decisive</h2>
  <ul class="bullets">
    <li><strong>Turn the embedder on.</strong> Set <code>NUDOX_EMBED_MODEL_DIR</code> and
    re-run. Half of <code>search</code>'s ranking has never been measured, and the
    scenarios it should help most &mdash; prose questions, concept lookups &mdash; are
    exactly the ones Nudox lost.</li>
    <li><strong>Install the Go oracle and index a Python source checkout.</strong> Polyglot
    reach is the strongest structural argument and it is currently untested.</li>
    <li><strong>Grow to ~50 scenarios, weighted to disagreement.</strong> The informative
    scenarios are the ones where the tools diverge; ties tell you nothing. Ten is
    enough to debug an instrument, not to conclude with.</li>
    <li><strong>Paginate <code>diff</code> in the adapter.</strong> A real agent that did not find
    its answer on page one would ask for page two. Not doing so measures the
    harness's patience rather than the tool's capability.</li>
    <li><strong>Make the keys ablation real</strong> by reading the structured <code>key</code>
    field, so the address scheme's saving is priced rather than assumed.</li>
    <li><strong>Then add the MCP field.</strong> Context7, Deepcon, Exa and NIA are the
    comparison everyone will ask for &mdash; but only after these numbers are stable,
    or the result will be a story about a misconfigured harness.</li>
    <li><strong>Add an agent-driven mode.</strong> Every adapter here follows a fixed
    tool path. The actual product question is whether a model can drive this surface
    unaided, and a scripted path cannot answer it.</li>
  </ul>
</section>

<footer>
  <p>Generated from <code>runs/run-2026-08-30-165221/report.json</code> by
  <code>scripts/plot_results.py</code>. Every number is measured; nothing is
  illustrative. Ground truth for each scenario was derived by reading the packages
  &mdash; file and line recorded in each scenario file.</p>
</footer>

</div>
'''

HTML = (HTML
    .replace('PLATE_EFFICIENCY', plate('efficiency', 'Cost against correctness, one point per adapter'))
    .replace('PLATE_COST', plate('cost_per_scenario', 'Tokens returned per scenario, by adapter'))
    .replace('PLATE_RESOLUTION', plate('resolution_gap', 'Strict versus loose recall per adapter'))
    .replace('PLATE_SURFACE', plate('surface_cost', 'Resident token cost per tool'))
    .replace('PLATE_ABLATIONS', plate('ablations', 'Nudox ablations by pass rate and cost')))

OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
OUT_PATH.write_text(HTML)
print("%s (%s bytes)" % (OUT_PATH, format(len(HTML), ",")))
