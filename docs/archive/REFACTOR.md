# Earley Parser Refactoring Checklist

## Audit Results Summary

After conducting a thorough 4-stage audit of the Earley parser components, we identified critical semantic issues that likely explain the current test suite failures (65 bootstrap errors out of 108 tests).

## Critical Issues Found

### ❌ Issue 1: Missing Nullable Handling in Predictor
**Location**: `src/parser.rs:546-576` (`predict()` function)
**Severity**: CRITICAL
**Impact**: Bootstrap grammar parsing failures

**Problem**: When predicting a nullable rule (one that can produce ε/empty), the predictor should immediately advance the parent task. Current implementation only registers continuations but doesn't handle immediate nullable completion.

**Current Code**:
```rust
// Lines 554-557: Only registers continuations
for child_alt_index in 0..child_rule.iter().count() {
    self.traces.register_waiting_parent_task(&name, child_alt_index, tid);
}
// Missing: Check if any alternative is nullable and advance parent immediately
```

**Required Fix**:
- [ ] Add nullable rule detection in `predict()`
- [ ] When nullable alternative found, create completion trace and advance parent task immediately
- [ ] Ensure proper interaction with continuation system

**Continuation Table Violation**: "Redundant PREDICT (nullable)" should add continuation ✅ Yes

---

### ❌ Issue 2: Dead/Incorrect Terminal Branch in Completer
**Location**: `src/parser.rs:532` (`complete()` function)
**Severity**: MEDIUM (likely dead code)
**Impact**: Potential logic confusion, incorrect MatchRec creation

**Problem**: Creates `MatchRec::Term('?', ...)` with placeholder character. This branch should never execute since terminals are handled by Scanner, not Completer.

**Current Code**:
```rust
Factor::Terminal(tmark, _ch) => MatchRec::Term('?', self.traces.get(tid).pos, tmark), // 🚨 BUG
```

**Analysis**: Either dead code or architectural confusion about Earley operation dispatch.

**Required Fix**:
- [ ] Investigate if this branch can ever be reached
- [ ] If unreachable, remove the Terminal branch entirely
- [ ] If reachable, fix the placeholder character issue
- [ ] Add assertion/panic to catch unexpected cases

---

## ✅ Components Working Correctly

### Main Parser Loop (lines 469-505)
- ✅ Proper initialization with root rule alternatives
- ✅ Correct dispatch logic (completed → complete, nonterminal → predict, terminal → scan)
- ✅ Trace size limit protection against infinite loops
- ✅ Standard FIFO queue processing with `pop_front()`

### Scanner Component (lines 578-605)
- ✅ Proper bounds checking before input access
- ✅ Correct continuation handling:
  - **Match**: Creates MatchRec, advances cursor, adds continuation via `queue_back` ✅
  - **No Match**: Silently drops task (PASS behavior) ✅
- ✅ Follows Earley algorithm specification

### Queue Management Strategy
- ✅ **PREDICTOR**: `queue_front` for depth-first exploration
- ✅ **SCANNER**: `queue_back` for breadth-first processing
- ✅ **COMPLETER**: `queue_back` for deferred parent continuation
- ✅ Proper LIFO/FIFO hybrid approach

---

## Continuation Passing Style Compliance

### Current State vs Required Behavior

| Operation | Required Continuation | Current Implementation | Status |
|-----------|----------------------|------------------------|---------|
| Successful SCAN | ✅ Yes | ✅ `task_advance_cursor` + `queue_back` | ✅ CORRECT |
| Successful PREDICT | ✅ Yes | ✅ Registers parent continuations | ✅ CORRECT |
| Successful COMPLETE | ❌ No | ✅ No direct continuation, resolves parents | ✅ CORRECT |
| Redundant PREDICT (nullable) | ✅ Yes | ❌ **MISSING** nullable advancement | ❌ BROKEN |
| Failed SCAN | ❌ No | ✅ Silently drops task | ✅ CORRECT |
| PASS (Terminal mismatch) | ❌ No | ✅ Silently drops task | ✅ CORRECT |

---

## Context: Why These Issues Matter

### Bootstrap Grammar Parsing
The iXML bootstrap grammar heavily uses nullable productions for optional elements:
- `s: (whitespace; comment)*.` - optional spacing (nullable)
- `prolog: version, s.` - version is optional in many contexts
- `(mark, s)?` patterns throughout the grammar

**Missing nullable handling** would cause the parser to fail on these fundamental iXML constructs, explaining the 65 bootstrap parsing errors.

### Test Suite Impact
- **Current**: 28/108 PASS (25.9%)
- **65 Bootstrap Errors** - Primary target for these fixes
- **15 Output Format Errors** - Lower priority
- **0 Parse Errors** - Input parsing works with valid grammars

---

## Refactoring Priority

### Phase 1: Critical Fixes
1. **Fix nullable handling in Predictor** - Should resolve majority of bootstrap errors
2. **Clean up Terminal branch in Completer** - Remove architectural confusion

### Phase 2: Validation
1. **Run test suite** after fixes
2. **Target improvement**: 25.9% → 60%+ pass rate
3. **Focus metric**: Reduce bootstrap errors from 65 to <20

### Phase 3: Architecture Review
1. **Review epsilon production handling** throughout codebase
2. **Ensure nullable rule detection** is consistent
3. **Add unit tests** for nullable prediction scenarios

---

## Implementation Notes

### Nullable Rule Detection
```rust
// Pseudo-code for nullable detection in predict()
if rule.is_nullable() {
    // Create immediate completion and advance parent
    let completion_id = self.traces.task_completed_epsilon(&name, alt_index, current_pos);
    // Trigger parent advancement immediately
    self.advance_waiting_parents(&name, completion_id);
}
```

### Continuation System Architecture
The parser uses TraceArena with MultiMap for parent-child relationships:
- **Parents register** to wait for specific `rule_name[alt_index]` completions
- **Children notify** via `get_waiting_parent_tasks_by_name()`
- **Advancement** via `task_advance_cursor()` with appropriate MatchRec

---

## Testing Strategy

### Minimal Test Cases for Nullable Handling
```rust
// Test case 1: Simple nullable
"optional: item?. item: 'a'." with input ""

// Test case 2: Nested nullable
"doc: (item, space)*, item. item: 'a'. space: ' '." with input "a"

// Test case 3: Bootstrap-style nullable
"rule: mark?, name. mark: '@'. name: 'x'." with input "x"
```

These should all parse successfully after nullable fixes.