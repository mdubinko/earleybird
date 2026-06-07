# Architecture

This document describes the major stages of earleybird and the design decisions
behind them. It is a map, not a reference — for details, read the code it points
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
            Arena<Content>  (indextree)
                    │  tree_to_test_format
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
  `**`, `++`, and groups become extra rules with reserved-prefix names (`--…`,
  `__eb_…`). The Earley core then only ever sees plain alternation and sequence.
  - *Tradeoff:* keeps the parser small, but synthesized rules leak into later
    stages — the tree-unpacker special-cases `--` names, and disambiguation depends
    on the order their alternatives are queued (see Stage 4).
- **Decision: validation is a separate pre-pass** (`src/validator.rs`) ahead of the
  bootstrap parse, plus BOM stripping at entry (errata E003).

### 2. Earley recognition — `Parser::parse` → `TraceArena`

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
  This replaced an earlier "One/Many" tagging scheme (removed in `65091b1`).

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
  keyed by `(name, origin, pos)`, not an O(|trace|) linear scan. This was the dominant
  real-world cost before 2026-06-06 (≈98% of the unicode_version grammar build; see
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

- *Tradeoff:* this exactly matches the catalog's tree on 889/890 cases with a
  fraction of the machinery of a full forest, but disambiguation is entangled with
  queue-scheduling order, and a *specific* enumerated tree is only reachable when it
  happens to coincide with the single retained derivation. The 890th case
  (`g12.c05`) is hyper-ambiguous and is scored conformant by a local suite override
  (accepted + flagged ambiguous; see `tests/suite-overrides.xml`) rather than exact
  match — see "single derivation vs. forest" below.

### 5. Serialization — `tree_to_test_format*` → XML string

`tree_to_test_format_recurse` walks the `Arena<Content>` and builds the XML string;
`validate_xml_output` checks name/char validity (XML well-formedness, not just ixml).
Variants thread through a version-mismatch flag and the ambiguity state.

## Paths not taken

Discernible from the repo and history:

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
- **Integer-interned symbols** *(prototyped, measured flat, reverted)*. Nonterminals
  are keyed by `SmolStr` throughout (`continuations`, `families`, task identity,
  `completed_by_span`). Interning to a bare `defn_order` offset `NonTermId(u32)` — the
  offset-identity principle from [Stage 2](#2-earley-recognition--parserparse--tracearena)
  applied to symbols — was the obvious candidate once the tree-extractor scan and
  hot-loop clones were fixed. It was tried (2026-06-06): swapping the per-item dedup
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

1. **Performance.** Most of this decision's tax has been paid down (all 2026-06-06):
   the dotted rule is shared via `Rc<Rule>` (advance and `predict`'s per-alt task
   creation are refcount bumps, not `Rule` clones); `matched_so_far` is an immutable
   `Rc` cons-stack (one node per advance, no vector regrow); the per-pop `Factor`
   clone and the per-`complete` waiting-parents `Vec` clone are gone; and
   `completed_trace` is span-indexed for extraction. What remains: nonterminal
   identity is still `SmolStr` (interning to `NonTermId` is the next lever), and
   `rule_reaches` recomputes static grammar reachability inside selection. Aggressive
   optimization pushes toward identity-only items + a side forest — i.e. unwinding
   this decision.
2. **Correctness under reordering.** Which derivation survives dedup, and which
   `filter_completed_trace` picks, both depend on queue order. Perf changes that
   reorder work can silently change the emitted tree (see the synthesized-rule
   ordering note above).
3. **Genuine disambiguation.** When a span is truly ambiguous, the non-chosen
   derivations no longer exist to choose from. This is why **g12.c05** cannot be made
   to reproduce any *specific* tree from the catalog's enumerated sample by selection
   alone — an SPPF/forest matter, not a selection bug. (It is not a suite failure:
   earleybird's output is spec-conformant — a valid tree, flagged ambiguous — and is
   scored as such via a local override; see `tests/suite-overrides.xml` and
   `log/g12.c05-explained.md`. A forest would only be needed to land on a *particular*
   enumerated tree, which the spec does not require.)

A plausible incremental path: promote `families` from signature-set to a real (even
minimal) forest by retaining the child edges per signature, move disambiguation into
an explicit walk over that forest (decoupled from scheduling), then intern symbols
(the remaining `SmolStr`-as-identity cost — see below). Completed-item indexing is
already done. This is a retrofit of the existing engine, not a rewrite.
