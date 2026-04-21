# Parser Statistics Design

## Design Principle: Compute on Access, Not on Update

**Store raw counters during parsing** (fast, no overhead)
**Compute derived statistics on access** (only when needed)

## Current Statistics

### Raw Counters (stored in ParseSession)
```rust
pub struct ParseSession {
    // Input metadata
    pub input_length: usize,

    // Operation tracking
    pub total_operations: u32,
    pub position_repeat_count: HashMap<usize, u32>,
    pub farthest_pos: usize,

    // Queue stats
    pub max_queue_size: usize,

    // Task tracking
    pub tasks_deduplicated: u32,  // Note: stored in TraceArena

    // Safety
    pub infinite_loop_threshold: u32,
}
```

### Derived Statistics (computed on access)
- **Deduplication rate**: `deduplicated / (created + deduplicated)`
- **Operations per position**: `total_ops / (farthest_pos + 1)`
- **Average queue size**: Would need `total_queue_samples` counter

## Proposed Additional Raw Counters

### High Priority (Low Overhead)

**1. Operation Type Counters**
```rust
pub predictor_ops: u32,    // Increment in predict()
pub scanner_ops: u32,      // Increment in scan()
pub completer_ops: u32,    // Increment in complete()
```
**Derived:** Operation percentages (predictor_ops/total_ops * 100)

**2. Scanner Effectiveness**
```rust
pub scanner_successes: u32,  // Scanner matched
pub scanner_failures: u32,   // Scanner didn't match
```
**Derived:** Scanner hit rate (successes / (successes + failures))

**3. Nullable Rule Handling**
```rust
pub nullable_predictions: u32,  // Predicted nullable rule
```
**Derived:** Nullable percentage (nullable_predictions / predictor_ops)

**4. Queue Sampling**
```rust
pub total_queue_samples: u32,  // How many times we sampled queue size
pub total_queue_size: u64,     // Sum of all queue sizes sampled
```
**Derived:** Average queue size (total_queue_size / total_queue_samples)

### Medium Priority (Moderate Overhead)

**5. Position-Specific Hotspots**
```rust
pub operations_per_position: Vec<u32>,  // Index = position
```
**Derived:**
- Max operations at any position
- Variance in operations (shows problematic positions)
- Only allocate if needed for debugging

**6. Rule Popularity** (Only if profiling enabled)
```rust
#[cfg(feature = "detailed-stats")]
pub predictions_by_rule: HashMap<SmolStr, u32>,
#[cfg(feature = "detailed-stats")]
pub completions_by_rule: HashMap<SmolStr, u32>,
```
**Derived:** Top 10 most-predicted/completed rules

### Low Priority (High Overhead, Debug Only)

**7. Parse Tree Stats** (computed during unpacking)
```rust
// Computed in unpack_parse_tree, not during parsing
pub parse_tree_nodes: usize,
pub parse_tree_depth: usize,
```

## Accessor Methods Pattern

```rust
impl ParseSession {
    /// Compute deduplication percentage from raw counters
    pub fn dedup_percentage(&self, deduplicated: u32, created: u32) -> f32 {
        if deduplicated == 0 { return 0.0; }
        (deduplicated as f32 / (created + deduplicated) as f32) * 100.0
    }

    /// Compute average operations per position
    pub fn avg_ops_per_position(&self) -> f32 {
        if self.farthest_pos == 0 { return self.total_operations as f32; }
        self.total_operations as f32 / (self.farthest_pos + 1) as f32
    }

    /// Compute scanner success rate
    pub fn scanner_success_rate(&self) -> f32 {
        let total = self.scanner_successes + self.scanner_failures;
        if total == 0 { return 0.0; }
        (self.scanner_successes as f32 / total as f32) * 100.0
    }

    /// Compute average queue size from samples
    pub fn avg_queue_size(&self) -> f32 {
        if self.total_queue_samples == 0 { return 0.0; }
        self.total_queue_size as f32 / self.total_queue_samples as f32
    }

    /// Get operation breakdown
    pub fn operation_breakdown(&self) -> (f32, f32, f32) {
        let total = self.total_operations as f32;
        if total == 0.0 { return (0.0, 0.0, 0.0); }
        (
            (self.predictor_ops as f32 / total) * 100.0,
            (self.scanner_ops as f32 / total) * 100.0,
            (self.completer_ops as f32 / total) * 100.0,
        )
    }
}
```

## Output Format Examples

### Minimal (Current)
```
📊 Parse stats: 411884 tasks created, 217247 deduplicated (34%), 386031 operations, max queue: 17
```

### Standard (Proposed)
```
📊 Parse stats: 411884 tasks, 217247 dedup (34%), 386031 ops, max queue: 17
   Operations: 45% predict, 23% scan (89% hit), 32% complete
```

### Detailed (--stats=detailed flag)
```
📊 Parse Statistics:
   Tasks: 411,884 created, 217,247 deduplicated (34.5%)
   Operations: 386,031 total (8,650 per position avg)
   Breakdown:
     • PREDICT: 173,714 (45.0%) - 12,345 nullable (7.1%)
     • SCAN:     88,807 (23.0%) - 78,999 success (89.0% hit rate)
     • COMPLETE: 123,510 (32.0%)
   Queue: max=17, avg=8.4
   Hotspot: position 3 (45,123 ops)
```

### Full Profile (--stats=profile)
```
📊 Parse Statistics:
   [Standard stats above]

   Top 10 Most Predicted Rules:
     1. --Expr.f-option4      23,456 (13.5%)
     2. --AndExpr.f-option1   19,234 (11.1%)
     3. OrExprSingle          15,678 (9.0%)
     ...

   Parse Tree: 442 nodes, depth=12, unpacking: 2.3ms
   Memory: 4.2MB trace arena, 156KB dedup set
```

## Implementation Priority

**Phase 1: Low-Hanging Fruit** (Add now)
- ✅ Tasks created/deduplicated (done)
- ⬜ Operation type counters (predictor_ops, scanner_ops, completer_ops)
- ⬜ Scanner success/failure counters
- ⬜ Nullable prediction counter

**Phase 2: Detailed Analysis** (Add if profiling)
- ⬜ Queue sampling (avg queue size)
- ⬜ Operations per position (hotspot detection)

**Phase 3: Deep Profiling** (Feature-gated)
- ⬜ Per-rule prediction/completion counts
- ⬜ Memory usage tracking
- ⬜ Parse tree metrics

## Key Design Points

1. **Zero allocation during parse loop** - just increment u32 counters
2. **Lazy computation** - only calculate percentages/averages when displaying
3. **Optional verbosity** - `--stats=minimal|standard|detailed|profile`
4. **Feature gates** - Heavy tracking behind `#[cfg(feature = "profile")]`
5. **No runtime cost** - When stats aren't needed, just u32 increments (nanoseconds)

## Memory Overhead

- **Minimal stats**: ~160 bytes (current + Phase 1)
- **Detailed stats**: +8KB (operations_per_position for 1K input)
- **Profile stats**: +~100KB (HashMap for per-rule tracking)

## Usage

```bash
# Current (always on)
cargo run --release -- parse -g grammar.ixml -i input.txt

# Standard stats (Phase 1 complete)
cargo run --release -- parse -g grammar.ixml -i input.txt --stats standard

# Detailed (Phase 2)
cargo run --release -- parse -g grammar.ixml -i input.txt --stats detailed

# Full profiling (Phase 3, requires feature)
cargo run --release --features profile -- parse -g grammar.ixml -i input.txt --stats profile
```
