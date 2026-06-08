# Architecture

This document describes the major stages of earleybird and the design decisions
behind them. It is a map, not a reference or a TODO — for details, read the code it points
at. For project rules and the ixml spec excerpt, see [AGENTS.md](AGENTS.md).

## Overview

earleybird turns an **ixml grammar** plus an **input string** into **XML**. It is
*self-hosting*: ixml grammars are themselves parsed by the same Earley engine that
parses end-user input, using a hand-coded bootstrap grammar.

```
ixml grammar text ──┐
                    ▼
            bootstrap grammar (hand-coded)         src/ixml_bootstrap.rs
                    │  Earley parse
                    ▼
            Grammar (rules + synthesized rules)     src/grammar.rs
                    │
input text ─────────┤  Earley parse                 src/parser.rs
                    ▼
            TraceArena  (Vec<Task>, one chosen derivation per item)
                    │  unpack_parse_tree
                    ▼
            Arena<Content>  (indextree, private build tree)
                    │  Document::from_content_arena
                    ▼
            Document  (owned output tree)            src/treebird.rs
                    │  to_xml
                    ▼
                  XML
```

## Stages

### 1. Grammar acquisition — `Grammar::from_ixml_str` → `Grammar`

The bootstrap grammar (`bootstrap_ixml_grammar`) is a hand-coded `Grammar` that
describes ixml's own syntax, mirroring the spec grammar rule-for-rule (comments in
`ixml_bootstrap.rs` reproduce each spec rule). User grammar text is parsed *by the
normal Earley parser* against this bootstrap grammar, then `Grammar::from_parse_tree`
walks the resulting tree to build the target `Grammar`.

- **Decision: self-hosting bootstrap rather than a generated/encoded parser.** One
  parsing engine to maintain and debug; the bootstrap grammar reads like the spec.
- **Decision: operators are desugared into synthesized rules.** `*`, `+`, `?`,
  `**`, `++`, and groups become extra rules with reserved-prefix (`--`) names
  (minted by `Grammar::mint_internal_id`). The Earley core then only ever sees
  plain alternation and sequence.
  - *Tradeoff:* keeps the parser small, but synthesized rules leak into later
    stages — the tree-unpacker special-cases `--` names, and disambiguation depends
    on the order their alternatives are queued (see Stage 4).
- **Decision: validation is a separate pre-pass** (`src/validator.rs`) ahead of the
  bootstrap parse, plus BOM stripping at entry (errata E003).

### 2. Earley recognition → `TraceArena`

A classic Earley loop (`predict` / `scan` / `complete`) driven by a
`PositionBucketedQueue` that enforces left-to-right, position-by-position
processing. Each Earley item is a `Task` in `TraceArena.arena` (a `Vec<Task>`
indexed by `TraceId`).

- **Decision: identity is a bare arena offset (allocate once, never shrink).** The
  set of Earley items only grows during a parse, so `arena` is a single append-only
  `Vec<Task>` and a task is *named by its offset* — `TraceId(usize)` — not by an
  allocated key. References between items are plain integers; there is no per-item
  identity allocation, and a slot, once written, is stable for the whole parse. The
  unifying rule: stable, monotonic data is addressed by offset, not by a hashed key.
  Nonterminal **definitions** have the same lifecycle (fixed at grammar-build time,
  never removed), so they are a candidate to extend the principle to — interning names
  to a bare `defn_order` offset. That was prototyped and reverted: it measured flat
  (rule names are short and already unique-by-`HashMap`), so it is recorded only as a
  possible future change — see [Paths not taken](#paths-not-taken).
- **Decision: scannerless, character-level.** Terminals match `char`s directly;
  there is no separate lexer. Fits ixml (which is defined over characters and has
  no token layer) at the cost of more items than a tokenized parser.
- **Decision: the item carries its own partial parse tree.** `Task.dot`
  (`DotNotation`) holds `matched_so_far` — the concrete children matched so far
  (terminal char, nonterminal span, or insertion). The item *is* the partial
  derivation.
  - *Tradeoff:* superb debuggability (an item shows exactly what it ate) and a
    simple unpacker, but items are large. The cloning this once implied is now
    structurally shared: the dotted rule is an `Rc<Rule>` and `matched_so_far` is an
    immutable `Rc` cons-stack (`MatchStack`), so a dot advance is one small node
    allocation plus refcount bumps, not a vector clone. This is the central design
    decision; its consequences appear in Stages 3–4 and in
    [Known tension](#known-tension-single-derivation-vs-forest).
- **Decision: dedup by identity hash** (`task_by_hash`). Items sharing
  `(name, alt_index, origin, pos, cursor_len)` are merged. The hash deliberately
  uses the cursor *length*, not the child contents, so it is cheap — but it means a
  second, structurally different derivation reaching the "same" item is dropped.
- **Decision: ambiguity is detected, not represented** (`families`). At every
  completion the derivation's *family signature* (`hash(alt_index, matched_so_far)`)
  is recorded per span. Two signatures at one span ⇒ structural ambiguity. The
  losing derivation's actual structure is *not* kept — only the fact that it existed.

### 3. Tree reconstruction — `unpack_parse_tree` → `Arena<Content>`

Starting from the root completion spanning the whole input, `unpack_parse_tree_internal`
recurses: at each span it picks **one** representative `Task` via
`filter_completed_trace`, emits an `Element` / `Attribute` / `Text` node into an
`indextree::Arena<Content>`, and recurses into that task's children. A path-scoped
`visited` set breaks cyclic grammars. A second pass fills `Attribute` values from
descendant text.

- **Decision: `indextree` for the output tree.** Arena-allocated `NodeId`s sidestep
  Rust ownership friction for a mutable tree and give a stable post-pass over all
  nodes (used for attribute assembly).
- **Decision: marks (`@`, `^`, `-`) and aliases are applied here**, not during
  recognition — the parse keeps the full structure; serialization decides what is an
  element, attribute, text, or muted.
- **Decision: `completed_trace` is span-indexed** (`completed_by_span`, built once
  before the recursion) so the per-node `filter_completed_trace` is an O(bucket) lookup
  keyed by `(name, origin, pos)`, not an O(|trace|) linear scan. Without the index that
  scan is the dominant real-world cost (≈98% of the unicode_version grammar build; see
  `docs/PROFILING.md`). Buckets preserve trace order so selection is unchanged.

### 4. Disambiguation — `filter_completed_trace` (+ `is_ambiguous`)

Because only one derivation per item survives Stage 2, choosing the output tree is a
*heuristic selection among retained top-level tasks*, not a walk over a forest:

- Synthesized (`--`) rules: first match in trace order — correct only because the
  queue pushes the zero-factor empty alternative first.
- User rules: lowest `alt_index`, except self-referential alternatives (whose first
  child re-enters the rule, directly or transitively — see `rule_reaches`) rank last
  to avoid needless nesting.

`is_ambiguous` / `span_is_ambiguous` separately read the `families` map to set
`ixml:state="ambiguous"` on output.

- *Tradeoff:* this reproduces the expected tree across the conformance suite with a
  fraction of the machinery of a full forest, but disambiguation is entangled with
  queue-scheduling order, and a *specific* enumerated tree is only reachable when it
  happens to coincide with the single retained derivation — see "single derivation
  vs. forest" below.

### 5. Serialization — `Document::to_xml` → XML string

At the public boundary the private `Arena<Content>` is converted to an owned
`treebird::Document` (`Document::from_content_arena`); serialization and output
validation then live on that owned tree (see ADR 6 below).

- **Decision: serialize from the owned tree, not the arena.** `Document::to_xml`
  (and the `ixml:state`-stamping `to_xml_with_state`) walk the `Document` to build
  the XML string. `Document::validate` checks XML well-formedness — name/char
  validity and the structural D-codes — which is distinct from ixml grammar validity.

## API design principles (ADRs)

These are cross-cutting decisions about the **public surface** — the crate's contract —
distinct from the pipeline decisions above. They exist because **1.0 freezes the public
API**. The meta-principle: *the public surface is the product; the engine is replaceable.*
Design the API as if the parser will be rewritten — the forest / Leo / native-repeat work
in [Paths not taken](#paths-not-taken) may do exactly that, and the surface must outlive
the internals.

### ADR 1: Model the domain, not the mechanism

Public types describe ixml's *output* (`treebird::Document` / `Node` — the XML infoset),
never the apparatus that produces it (`indextree::Arena`, `Content`, `TraceId`, Earley
`Task`s). *Litmus: would this type survive an engine rewrite?* `Document` survives;
`Arena<Content>` does not — so `Content` and the arena are implementation detail, and
`Document` is the contract.

### ADR 2: A dependency in a signature is a dependency in the contract

A dependency named in a public signature becomes one every consumer must add and
version-match, and a major bump in *that* crate forces a major bump in *ours*. `indextree`
must never appear in a public signature — `Parser::parse` returns the owned `Document`,
hiding indextree entirely. *Litmus: can a consumer call this without adding a dependency we
forced on them?*

### ADR 3: `pub` is a forever-promise; default private, promote with a reason

The test is not "could this be useful?" (everything could) but "are we willing to support
this exact shape until the next major?". Three tiers, chosen by *audience*: `pub` = the
contract (consumers); `pub(crate)` = cross-module internal; `#[doc(hidden)] pub` /
feature-gated = reachable-but-not-promised. The `eb` binary and the test scaffolding
(`testsuite_utils`, `test_grammars`, `alloc_count`) are internal couplings, not contract —
tier 3, not tier 1.

### ADR 4: Pre-1.0 is for breaking; 1.x is for adding

0.x exists to make the breaking calls while they are cheap, so 1.0 freezes the *right*
shape. "Keep an additive sibling so we don't break anyone" is a 1.x discipline and is wrong
at 0.x: collapse to the one true shape before the freeze rather than carrying transitional
duplicates across it. (Concretely: `Parser::parse` returns `Document` as the single
entry and the arena path is `pub(crate)` — collapsed before 1.0 rather than kept beside
an additive `parse_to_document` sibling.)

### ADR 5: One obvious way per task

Two public functions that both "parse" but return different types is a smell: consumers
must learn which to use and we maintain both forever. Prefer one canonical entry returning
the rich type, with everything else as *adapters on that type* (`Document::to_xml`, later
`to_json`). *Litmus: is this a new capability, or a second road to an existing one?* Second
roads get cut.

### ADR 6: Freeze the model, keep serialization plural; the type owns its invariants

The infoset (`Document` / `Node`) is the stable core; XML, JSON, … are additive serializers
*over* it, not variant models. The type that owns the data owns its well-formedness — so
XML output validation (the D-codes) belongs on `Document` / `to_xml`, not scattered across
the parser on `Arena<Content>`.

The public surface is both tiered (ADR 3) and collapsed (ADR 4):
the contract is `Parser::parse(&str) -> Result<Document, ParseError>`, the `treebird` types
(`Document` / `Node`, `to_xml`, `validate`), and `ParseError`. The arena path is fully
private — `Content`, `Parser::parse_to_arena` (used by the grammar bootstrap), and
`Grammar::from_parse_tree` are `pub(crate)`; the Earley internals (`Task`, `TraceArena`, …)
likewise; the `ixml:state`-stamping `to_xml_with_state` is `#[doc(hidden)]`.

XML output validation lives on the owned tree (ADR 6) as `Document::validate` — **D02**
(duplicate attribute), **D03** (non-XML name), **D04** (bad character), **D06** (one
top-level element), **D07** (reserved `xmlns`). The one exception is **D05** (attribute at
the document root): it is *unrepresentable* on `Document` (attributes belong to elements,
so a root attribute would be silently dropped by `from_content_arena`), so it is enforced
one step earlier, by `Parser::parse` at the arena boundary, before the `Document` is built.
This "make illegal states unrepresentable, enforce the rest at the boundary" split is the
concrete shape ADR 6 takes here.

## Paths not taken

- **A shared packed parse forest (SPPF).** The field-standard pipeline
  (recognizer → SPPF → disambiguating walk; cf. Scott 2008, and Markup Blitz /
  CoffeePot among ixml processors) was not built. earleybird instead fuses children
  into the Earley item and keeps a single derivation. The `families` map is the seed
  of a forest — it records *that* alternatives exist but discards their structure.
  Moving to a real forest is recorded as under consideration.
- **GLL engine.** Considered as an alternative to retrofitting Earley; not adopted.
- **tree-sitter / external engines.** `experimental/treesitter-ixml/` and
  `docs/archive/xrust_experiment.rs` are abandoned scaffolds; the project committed
  to a from-scratch Earley core.
- **Integer-interned symbols** *(prototyped, measured flat)*. Nonterminals
  are keyed by `SmolStr` throughout (`continuations`, `families`, task identity,
  `completed_by_span`). Interning to a bare `defn_order` offset `NonTermId(u32)` — the
  offset-identity principle from [Stage 2](#2-earley-recognition--tracearena)
  applied to symbols — was the obvious candidate once the tree-extractor scan and
  hot-loop clones were fixed. When tried, swapping the per-item dedup
  hash from the name to the id was **flat**. Rule names are short (`SmolStr`, usually
  inline) and already unique-by-`HashMap`, so SipHash over the name was never the
  dominant per-item cost — and a half-interned state even *adds* a name→id lookup. A
  real win would require going all the way (definitions as a `Vec<BranchingRule>`,
  `Factor` carrying the id, no name lookups at all), which is an architectural cleanup,
  not a measured speedup. Not pursued; the algorithmic lever (Leo, the O(n²) item
  count) is the better next target.

## Known tension: single derivation vs. forest

The keystone decision — *the Earley item is the (one) partial parse tree* — buys
simplicity and debuggability and fights back in three places:

1. **Performance.** Most of this decision's tax is paid down structurally: the
   dotted rule is shared via `Rc<Rule>` (advance and `predict`'s per-alt task
   creation are refcount bumps, not `Rule` clones); `matched_so_far` is an immutable
   `Rc` cons-stack (one node per advance, no vector regrow); and `completed_trace`
   is span-indexed for extraction. What remains: nonterminal identity is still
   `SmolStr` (interning to `NonTermId` is the next lever), and `rule_reaches`
   recomputes static grammar reachability inside selection. Aggressive optimization
   pushes toward identity-only items + a side forest — i.e. unwinding this decision.
2. **Correctness under reordering.** Which derivation survives dedup, and which
   `filter_completed_trace` picks, both depend on queue order. Perf changes that
   reorder work can silently change the emitted tree (see the synthesized-rule
   ordering note above).
3. **Genuine disambiguation.** When a span is truly ambiguous, the non-chosen
   derivations no longer exist to choose from, so a *specific* tree among a
   hyper-ambiguous input's many valid trees may not be the one retained (the suite's
   `g12.c05` is one such case). This is an SPPF/forest matter, not a selection bug;
   the spec leaves the choice of tree among ambiguous parses undefined.

Tensions 2 and 3 share a root cause — only one derivation is kept — and resolving them
means growing `families` from an ambiguity-detector into a real (even minimal) parse
forest and selecting by an explicit walk of it. That is a retrofit of the existing engine,
not a GLL-style rewrite; the concrete steps are tracked in `TODO.txt`.
