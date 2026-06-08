use argh::FromArgs;
use earleybird::{grammar::Grammar, parser::Parser};
use std::path::PathBuf;
use std::time::Instant;

/// One benchmark family: a fixed grammar plus an input generator parameterized by N.
/// Running the same family across increasing N exposes the parser's scaling curve,
/// which is what reveals super-linear (quadratic/cubic) behavior in the engine.
struct BenchFamily {
    name: &'static str,
    grammar: &'static str,
    /// Build the input string for a given size N.
    make_input: fn(usize) -> String,
}

/// A "heavy" real-world stress case: a fixed grammar+input loaded from the ixml
/// test corpus, not parameterized by N. These distill pathologies the synthetic
/// families don't reach (e.g. huge Unicode character-class alternations producing
/// hundreds of thousands of Earley tasks over a tiny input). Skipped by default
/// because each run takes seconds; enable with --heavy or a matching --filter.
struct HeavyCase {
    name: &'static str,
    grammar_path: &'static str,
    input_path: &'static str,
}

fn heavy_cases() -> Vec<HeavyCase> {
    vec![
        // The Unicode-version diagnostic: 84-byte input, ~578k tasks created.
        // Used to run >5min and crash; now ~35s. A near-ideal algorithmic target.
        HeavyCase {
            name: "unicode_version",
            grammar_path: "ixml/tests/correct/unicode-version-diagnostic.ixml",
            input_path: "ixml/tests/correct/unicode-version-diagnostic.txt",
        },
        // ixml parsing itself: the ixml grammar applied to (essentially) its own
        // source text. Small grammar to build, but the ~2KB grammar-text input is
        // the meaningful parse load — the mirror profile of unicode_version.
        // Moved here out of the conformance suite (the "ixml" test-set).
        HeavyCase {
            name: "ixml_self",
            grammar_path: "ixml/tests/ixml/ixml.ixml",
            input_path: "ixml/tests/ixml/ixml.inp",
        },
    ]
}

fn families() -> Vec<BenchFamily> {
    vec![
        // Right recursion: stresses the completer the hardest in classic Earley.
        BenchFamily {
            name: "right_recursion",
            grammar: r#"doc = "a", doc | "a"."#,
            make_input: |n| "a".repeat(n),
        },
        // Left recursion: the mirror image; completions chain differently.
        BenchFamily {
            name: "left_recursion",
            grammar: r#"doc = doc, "a" | "a"."#,
            make_input: |n| "a".repeat(n),
        },
        // Synthesized repetition path (the `+` operator).
        BenchFamily {
            name: "repeat_plus",
            grammar: r#"doc = "a"+."#,
            make_input: |n| "a".repeat(n),
        },
        // Balanced nesting: depth-N parentheses around a single terminal.
        BenchFamily {
            name: "nested",
            grammar: r#"doc = "(", doc, ")" | "x"."#,
            make_input: |n| format!("{}x{}", "(".repeat(n), ")".repeat(n)),
        },
        // Highly ambiguous: O(n^3) territory. Keep N modest.
        BenchFamily {
            name: "ambiguous",
            grammar: r#"doc = doc, doc | "a"."#,
            make_input: |n| "a".repeat(n),
        },
    ]
}

#[derive(FromArgs)]
/// Run parser micro-benchmarks across input sizes to track performance and scaling.
#[argh(subcommand, name = "bench")]
pub struct Bench {
    /// comma-separated input sizes (N) to test (default: 8,16,32,64,128)
    #[argh(option, long = "sizes", default = "String::from(\"8,16,32,64,128\")")]
    sizes: String,

    /// only run benchmark families whose name contains this substring
    #[argh(option, long = "filter")]
    filter: Option<String>,

    /// repetitions per measurement; reports min and median (default: 3)
    #[argh(option, long = "reps", default = "3")]
    reps: usize,

    /// also run heavy real-world cases loaded from ixml/ (slow; e.g. unicode_version)
    #[argh(switch, long = "heavy")]
    heavy: bool,

    /// optional CSV output path
    #[argh(option, short = 'o', long = "csv")]
    csv: Option<String>,

    /// show parser task statistics while benchmarking (noisy, but useful when inspecting one case)
    #[argh(switch, long = "stats")]
    stats: bool,
}

impl Bench {
    pub fn run(self) {
        let sizes = parse_sizes(&self.sizes);
        if sizes.is_empty() {
            eprintln!("No valid sizes parsed from '{}'", self.sizes);
            std::process::exit(1);
        }
        let reps = self.reps.max(1);

        println!(
            "{:<18} {:>6} {:>9} {:>10} {:>10}  status",
            "family", "n", "in_len", "min_ms", "med_ms"
        );
        println!("{}", "-".repeat(66));

        let mut csv_rows: Vec<String> = Vec::new();

        for fam in families() {
            if let Some(f) = &self.filter {
                if !fam.name.contains(f.as_str()) {
                    continue;
                }
            }

            let grammar = match build_grammar(fam.grammar, self.stats) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("grammar build failed for {}: {}", fam.name, e);
                    continue;
                }
            };

            for &n in &sizes {
                let input = (fam.make_input)(n);
                let input_len = input.chars().count();

                let mut durations = Vec::with_capacity(reps);
                let mut status = "ok";
                for _ in 0..reps {
                    // Fresh parser per run (matches suite behavior). Grammar clone is
                    // deliberately outside the timed region so we measure parsing only.
                    let mut parser = Parser::new(grammar.clone());
                    parser.set_stats_enabled(self.stats);
                    parser.set_phase_report(self.stats);
                    let start = Instant::now();
                    let res = parser.parse(&input);
                    durations.push(start.elapsed());
                    if res.is_err() {
                        status = "ERR";
                    }
                }
                durations.sort();
                let min_ms = durations.first().unwrap().as_secs_f64() * 1000.0;
                let med_ms = durations[durations.len() / 2].as_secs_f64() * 1000.0;

                println!(
                    "{:<18} {:>6} {:>9} {:>10.3} {:>10.3}  {}",
                    fam.name, n, input_len, min_ms, med_ms, status
                );
                csv_rows.push(format!(
                    "{},{},{},{},{:.3},{:.3},,,{}",
                    "synthetic",
                    csv_field(fam.name),
                    n,
                    input_len,
                    min_ms,
                    med_ms,
                    status
                ));
            }
            println!();
        }

        // Heavy real-world cases (fixed grammar+input from the corpus). Run when
        // --heavy is set, or when an explicit --filter names one. Always 1 rep:
        // these take seconds, so a single sample is plenty for tracking trends.
        for hc in heavy_cases() {
            let matches_filter = match &self.filter {
                Some(filter) => hc.name.contains(filter.as_str()),
                None => true,
            };
            if !matches_filter {
                continue;
            }
            if !self.heavy && self.filter.is_none() {
                println!(
                    "(skipping heavy case '{}'; run with --heavy or --filter {})",
                    hc.name, hc.name
                );
                continue;
            }

            let grammar_src = match std::fs::read_to_string(hc.grammar_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("skip heavy '{}': {} ({})", hc.name, e, hc.grammar_path);
                    continue;
                }
            };
            let input = match std::fs::read_to_string(hc.input_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("skip heavy '{}': {} ({})", hc.name, e, hc.input_path);
                    continue;
                }
            };
            // Time grammar construction and input parsing separately. For this
            // corpus the cost lives in grammar construction (the bootstrap parser
            // chewing through huge Unicode character-class definitions), so the
            // two-phase split is the whole point.
            let g_len = grammar_src.chars().count();
            let build_start = Instant::now();
            let grammar = match build_grammar(&grammar_src, self.stats) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("grammar build failed for {}: {}", hc.name, e);
                    continue;
                }
            };
            let build_ms = build_start.elapsed().as_secs_f64() * 1000.0;

            let input_len = input.chars().count();
            let mut parser = Parser::new(grammar);
            parser.set_stats_enabled(self.stats);
            parser.set_phase_report(self.stats);
            let parse_start = Instant::now();
            let status = if parser.parse(&input).is_err() {
                "ERR"
            } else {
                "ok"
            };
            let parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;

            println!(
                "{:<18}  grammar_len={:<6}  build={:>10.3}ms   input_len={:<4}  parse={:>9.3}ms   {}",
                hc.name, g_len, build_ms, input_len, parse_ms, status
            );
            csv_rows.push(format!(
                "{},{},,{},{},{},{:.3},{:.3},{}",
                "heavy",
                csv_field(hc.name),
                input_len,
                "",
                "",
                build_ms,
                parse_ms,
                status
            ));
            println!();
        }

        if let Some(path) = &self.csv {
            let path = PathBuf::from(path);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            let mut body = String::from(
                "kind,name,n,input_len,min_parse_ms,median_parse_ms,build_ms,parse_ms,status\n",
            );
            body.push_str(&csv_rows.join("\n"));
            body.push('\n');
            match std::fs::write(&path, body) {
                Ok(()) => println!("Results written to: {}", path.display()),
                Err(e) => eprintln!("Could not write CSV {}: {}", path.display(), e),
            }
        }
    }
}

fn parse_sizes(s: &str) -> Vec<usize> {
    s.split(',')
        .filter_map(|x| x.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .collect()
}

fn build_grammar(source: &str, stats: bool) -> Result<Grammar, earleybird::parser::ParseError> {
    if stats {
        // Profiled build prints the bootstrap parse's per-phase breakdown — on heavy
        // grammars this build is the dominant cost (see docs/PROFILING.md).
        Grammar::from_ixml_str_profiled(source)
    } else {
        Grammar::from_ixml_str_quiet(source)
    }
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
