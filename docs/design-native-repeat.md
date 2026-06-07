# Design note: native repeat operators in the Earley engine

Status: **proposed** (2026-06-06). No code yet. Branch: `feat/native-repeat-operators`.
Supersedes the perf priority of nonterminal interning (measured flat) and overlaps
Leo's optimization (see [Relation to other work](#relation-to-other-work)).

## TL;DR

Stop desugaring `?` `*` `+` `**` `++` into synthesized right-recursive rules; handle
repetition **natively** in the Earley item/dot model. Measured prize: synthesized
(`--`) rules are **~50% of all Earley items** on real grammars and **97.7%** on
repeat-heavy ones. The desugaring is *right-recursive*, which is Earley's O(n²) item
pathology; native repetition keeps the dot on one looping factor slot so per-iteration
items share identity and the chain collapses toward **O(n)**.

## Motivation (measured, not assumed)

Every per-op optimization we tried this cycle measured **flat** (nonterminal interning
hash swap; nesting `continuations` to drop `SmolStr` key construction; synth-name
length — 0% of 390,905 task-name constructions even exceed the 23-byte `SmolStr` inline
limit). Reason: the cost is dominated by **item count**, not per-item constant cost
(see docs/PROFILING.md and the methodology's repeated lesson).

Item-count attribution — fraction of stored Earley items that are synthesized (`--`)
repeat/group rules (throwaway counter in `save_task`, 2026-06-06):

| workload | stored items | synth (`--`) | synth % |
| --- | --- | --- | --- |
| unicode_version (grammar build) | 577,883 | 297,036 | **51.4%** |
| ixml_self (input parse) | 231,434 | 116,913 | **50.5%** |
| repeat_plus n=128 (input parse) | 17,594 | 17,190 | **97.7%** |

Synthetic scaling already shows the pathology: `repeat_plus ~O(n^2.5)`,
`right_recursion ~O(n^2.6)` (docs/PROFILING.md). `repeat_plus` is *entirely* synth
items, so its super-linear blowup is exactly the repeat desugaring.

Caveat: `--` synth rules include **group** rules `(...)` as well as repeats, so native
repeat alone does not eliminate all of them — but repeats are the bulk, and groups can
follow the same inlining approach (see [Groups](#groups-the-hard-part)).

## Background: what the desugaring does today

`src/grammar.rs` (`SeqBuilder::repeat0/repeat1/opt` + the `-sep` variants) rewrites at
grammar-build time, minting names via `mint_internal_id("f-star")` →
`--{rulename}.{hint}{nid}`:

```
f?      ⇒  --r.f-option:  f | ().
f*      ⇒  --r.f-star:    (f, --r.f-star)?.      (right-recursive)
f+      ⇒  --r.f-plus:    f, --r.f-star.          → f, then right-recursive star
f++sep  ⇒  --r.f-plus-sep: f, (sep, f)*.
f**sep  ⇒  --r.f-star-sep: (f++sep)?.
```

These synth rules are `Mark::Mute` (names start with `-`), so their matched children
**flatten** into the parent in the output tree (the unpacker skips muted rules). The
parser also special-cases them: `parser.rs` `filter_completed_trace` returns the
first trace for any `name.starts_with("--")`, and `is_self_ref` detects operator
desugaring when ranking derivations.

## Why native repetition cuts item count

Right recursion (`A → x, A`) makes Earley record a distinct item per origin: completing
the inner `A` that started at each position resumes a different parent, so the item set
is O(n²) on the recursion depth. `--r.f-star` is precisely this shape, applied once per
repeated element across the input — hence `repeat_plus`'s 97.7%-synth, O(n^2.5) profile.

Handled natively, the dot **does not leave the parent rule**: it sits on the repeat
factor, predicts the inner, and on inner completion either loops (predict inner again at
the new position) or advances past the factor (once `min` is met). Crucially the looping
item keeps the **same** `(rule, alt, dot-index, origin)` identity across iterations, so
dedup folds the per-iteration explosion. No separate Earley set keyed by a synth name,
no right-recursive chain — the common `*`/`+` case trends to **O(n)** items.

## Design

### Grammar representation
Add a repeat factor instead of desugaring:

```rust
enum Factor {
    Terminal(TMark, TerminalDefn),
    Nonterm(Mark, SmolStr, Option<SmolStr>),
    Insertion(TMark, SmolStr),
    Repeat(Box<RepeatFactor>),            // NEW
}

struct RepeatFactor {
    inner: RepeatInner,        // a single Factor, or a group (sequence of Factors)
    min: u32,                  // 0 for * and ?, 1 for + and ++
    max: Option<u32>,          // Some(1) for ?, None for * + ** ++
    sep: Option<RepeatInner>,  // Some(..) for ** and ++
}

enum RepeatInner { Factor(Factor), Group(Rc<Rule>) }  // Group reuses the Rule sequence repr
```

Keep the existing desugaring path behind a flag as a **fallback** during rollout so the
suite never regresses on not-yet-native cases (hybrid mode).

### Dot / item state
`DotNotation` currently advances factor-by-factor over `iteratee.factors`. When the dot
is *on* a `Repeat`, it needs a small sub-state. Minimal viable encoding:

- `dot-index` points at the repeat factor.
- a per-item `repeat phase`: `Before` (about to try an iteration), `AfterInner` (matched
  an inner, may take a separator then loop, or finish), and a `count` (only needed up to
  `min`/`max`; clamp so identity stays coarse — e.g. store `min(count, min)` so all
  counts ≥ min share identity and dedup collapses).
- For a **group** inner, a nested dot over the group's `Rule` sequence (an inner
  `MatchStack` segment).

Identity (the dedup hash) must include the repeat phase and clamped count, but **not** an
unbounded raw count — that's what makes iterations dedup.

### Engine ops
- **predict:** dot on `Repeat` in `Before` (or `AfterInner` with sep matched) → predict
  `inner` at the current position.
- **complete:** inner completes → record its child in the parent item's `MatchStack` at
  the repeat slot; emit two successors: (a) **loop** (back to `Before`/sep-expecting) and
  (b) **advance past** the repeat factor, *iff* `count >= min`. For `?`/bounded `max`,
  suppress the loop once `max` reached.
- **scan:** unchanged for terminal inners (the inner is just a factor).
- **separators:** between iterations, require `sep` (`Before` after first inner becomes
  "expect sep, then inner"). `**` = `(f ++ sep)?` semantics (zero allowed); `++` requires
  ≥1.
- **nullable inner** (`()*`): reuse the existing position-repeat infinite-loop guard so a
  zero-width inner cannot loop forever at one position.

### Tree extraction (unpack)
The repeat slot holds a **variable number** of inner children. They must splice into the
parent's child list **in order** — which is exactly today's mute-synth flattening, just
without the nonterminal boundary. `MatchStack`/`matches_in_order` already yield children
oldest-first; the repeat slot contributes its N recorded inner matches inline. Group
inners contribute their sub-sequence's children (also flattened). Marks: the repeat
itself is transparent; marks on the inner factor/group apply to the inner as today.

### Disambiguation
Removing `--` rules deletes the `starts_with("--")` first-match shortcut and the
`is_self_ref` operator-desugaring detection in `filter_completed_trace`. The native
construct needs its own (simpler) rule: a repeat has one canonical derivation per child
split; genuine ambiguity inside the inner is handled by the existing `families`
machinery. Re-derive and re-test the g05/g06/g10/g12 family carefully.

## Groups: the hard part
`(a, b)*` has a multi-factor `inner`, so it needs a nested dot over a sub-sequence. Model
the group as an `Rc<Rule>` and run an inner dotted sequence whose completion feeds the
outer repeat. This is effectively inlining the synth rule's *body* without the synth
*nonterminal* (no separate Earley set, no right-recursion) — still a win, but it is the
fiddliest piece and should come after single-factor repeats prove out.

## Phased plan (each phase: build, full release suite 890/890, `cargo test`, measure, commit)
1. **Design spike:** add `Factor::Repeat` repr + grammar build for single-factor `*`/`+`
   only; keep desugaring fallback for everything else (hybrid). No engine changes yet —
   just confirm the grammar carries the construct and round-trips.
2. **Engine: single-factor `*`/`+`, no sep/group.** Implement predict/complete loop +
   unpack flattening + dedup identity. **Measure `repeat_plus` item count and wall**
   (target: O(n²)→O(n); confirm with `--stats` task counts across n=64/128/256 and
   `--features alloc-count`). Gate suite.
3. **`?` and bounded `max`.**
4. **Separators `**` / `++`.**
5. **Groups `(...)*` etc.** (nested inner dot).
6. **Remove the desugaring fallback** once all forms are native and conformance holds;
   delete the `--` synth-rule special-casing in the disambiguator.

Stop after any phase if item count does **not** drop as predicted — that falsifies the
thesis for that form and is a signal, not a rounding error (per docs/PROFILING.md).

## Relation to other work
- **Leo's optimization** ([[project_item_set_explosion]]): the general fix for
  right-recursion (incl. user-written `A = A, x`). Native repeat is a *targeted* fix for
  the desugared-repeat subset — the overwhelmingly common real-world case — and is likely
  simpler to land. Leo remains the fallback for arbitrary right recursion.
- **Nonterminal interning**: shelved (measured flat); native repeat removes ~half the
  nonterminal *instances* anyway, which is the better way to cut their cost.
- **SPPF / parse forest**: orthogonal (ambiguity *representation*). Native repeat changes
  the recognizer's item set, not the forest decision.

## Open questions
- Exact identity encoding for the repeat phase/count that maximizes dedup without losing
  derivations (the crux for the O(n) collapse).
- Whether group inners should reuse `Rc<Rule>` directly or get a dedicated nested-dot type.
- Interaction with insertions and marks inside repeated groups (conformance corners).
- Does removing synth rules perturb the queue-order-dependent derivation selection
  (the synthesized-rule first-match dependency, [[project_synth_rule_dependency]])?
