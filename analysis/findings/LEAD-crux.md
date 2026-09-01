# LEAD — why the exact name match vanished, with the arithmetic

## Header
- agent: LEAD (session lead, verifying E2's handoff and closing E1's question)
- question: Why does an exact, unique name match disappear from `search` when the embedder is on?
- files opened: `/home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/hits.rs:111-205,280-292,460-470`, `search/mod.rs:570-576`, `mcp/tools/mod.rs:135-146,308-318,450-470`
- read budget used: ~120 lines
- verdict on my question: RESOLVED

## Claims

### CL.1 — A hit's score is relevance multiplied by a visibility weight and a kind weight
- status: READ
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/hits.rs:184-186
- quote:
  ```rust
  pub(crate) fn score_of(relevance: f32, visibility: Visibility, kind: KindDiscriminant) -> f32 {
      relevance * visibility_weight(visibility) * kind_weight(kind)
  }
  ```
- bears on: A2
- would falsify: a second scoring path that bypasses `score_of` for one of the sections.

### CL.2 — The name section's relevance is 1.0 for an exact match, and it is then multiplied by visibility
- status: READ
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/hits.rs:111-117,286-290
- quote:
  ```rust
  fn text_relevance(entry_key: &str, prefix_lower: &str) -> f32 {
      if entry_key == prefix_lower {
          1.0
      } else {
          prefix_lower.len() as f32 / entry_key.len().max(1) as f32
      }
  }
  ```
- bears on: A2, A3
- would falsify: the name section scoring exact matches through a different function.

### CL.3 — A crate-private symbol is weighted 0.6 against a public symbol's 1.0
- status: READ
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/hits.rs:137-148
- quote:
  ```rust
  fn visibility_weight(visibility: Visibility) -> f32 {
      match visibility {
          // Reachable from anywhere. The reference point.
          Visibility::Public => 1.0,
          // Reachable from subtypes of the declaring type.
          Visibility::Protected => 0.75,
          // Reachable within one compilation unit (crate / assembly / package).
          Visibility::Internal | Visibility::Package | Visibility::Crate => 0.6,
          // Reachable only inside the declaring scope.
          Visibility::Private => 0.4,
      }
  }
  ```
- bears on: A2
- would falsify: `search_slice_with_raw` being Public rather than crate-visible.

### CL.4 — Semantic relevance is `(cosine + 1) / 2`, so an unrelated symbol scores 0.5, not 0
- status: READ
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/mod.rs:573-576
- quote:
  ```rust
  fn cosine_to_relevance(cosine: f32) -> f32 {
      let clamped = cosine.clamp(-1.0, 1.0);
      f32::midpoint(clamped, 1.0).max(f32::EPSILON)
  }
  ```
- bears on: A1, A2
- would falsify: a caller rescaling this before `score_of`.

### CL.5 — Sections are merged by score alone, with no section-priority term
- status: READ
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/mcp/tools/mod.rs:466
- quote:
  ```rust
  deduped.sort_by(|(ka, a), (kb, b)| b.score.total_cmp(&a.score).then_with(|| ka.cmp(kb)));
  ```
- bears on: A2, F1
- would falsify: a section field participating in the comparator.

### CL.6 — Therefore any public symbol with cosine > 0.2 outranks a perfect crate-private name match
- status: INFERRED
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/hits.rs:184-186
- quote:
  ```rust
  pub(crate) fn score_of(relevance: f32, visibility: Visibility, kind: KindDiscriminant) -> f32 {
      relevance * visibility_weight(visibility) * kind_weight(kind)
  }
  ```
- bears on: A2, F1
- reasoning: Composing CL.1–CL.5 for the measured query. The exact match
  `pub(crate) unsafe fn search_slice_with_raw` scores
  `1.0 × 0.6 × 1.0 = 0.60`. A public function or impl scores
  `((cos + 1) / 2) × 1.0 × 1.0`, which exceeds 0.60 whenever `cos > 0.2`.
  Since CL.5 merges the two sections on score alone, and the page is 50 rows
  (`DEFAULT_LIMIT`, `mcp/tools/mod.rs:137`), the exact match is pushed off the
  page as soon as fifty public symbols in a 2,129-symbol crate clear cosine
  0.2 — which the measured output shows happening, since the returned rows are
  public `find_raw` functions and public `impl` blocks.
- to promote to READ: a test asserting the relative order of a crate-private
  exact match and a public semantic hit, or an instrumented run printing both
  scores for this query.
- would falsify: `search_slice_with_raw` being Public; or the name section
  being fetched with its own reserved slots before the merge.

### CL.7 — The two relevance scales are not commensurable
- status: INFERRED
- evidence: /home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/search/mod.rs:570-576
- quote:
  ```rust
  /// bottom keeps the result strictly positive, because a `0.0` score would make
  /// `score_of`'s product zero regardless of visibility and collapse every
  /// maximally-dissimilar row into one tie.
  fn cosine_to_relevance(cosine: f32) -> f32 {
      let clamped = cosine.clamp(-1.0, 1.0);
      f32::midpoint(clamped, 1.0).max(f32::EPSILON)
  }
  ```
- bears on: A1, A2
- reasoning: The name section spends its relevance range on *how much of the
  key the query matched* — an exact match is 1.0, a prefix is a length ratio.
  The semantic section spends its range on cosine mapped through a midpoint,
  so an orthogonal symbol lands at 0.5 and an actively dissimilar one still
  lands above zero. A floor of 0.5 for "unrelated" sits above the 0.4 a
  private exact match receives and just under the 0.6 a crate-private one
  receives. The quoted comment shows the floor is deliberate — chosen to avoid
  a tie at zero — but its side effect is that the two sections' numbers are
  not on the same scale, and CL.5 compares them as if they were.
- to promote to READ: a documented statement of intended cross-section
  comparability, or a normalisation step between the sections.
- would falsify: a rescaling of either section before the merge.

## Unanswered
- Whether the name section is *fetched* at all for this query under the
  embedder-on path, or fetched and then outranked. CL.5 makes outranking
  sufficient to explain the measurement, but E1 owns confirming the fetch.

## Out of scope, noticed
- `MAX_LIMIT` clamps a caller-supplied `0` up to 500 rather than treating it as
  unlimited (`mcp/tools/mod.rs`), which is a trap for a caller expecting
  "no limit".
- A structural-only entry point already exists: the `section != SECTION_SEMANTIC`
  guard at `mcp/tools/mod.rs:313`.
