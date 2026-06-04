use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugLevel {
    Off,
    Basic,
    Detailed,
    Trace,
}

#[derive(Debug, Clone)]
pub struct DebugConfig {
    pub level: DebugLevel,
    pub position_filter: Option<usize>,
    pub failure_only: bool,
    pub trace_file: Option<String>,
    pub enabled_categories: Option<HashSet<String>>,
}

impl DebugLevel {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "off" => Ok(DebugLevel::Off),
            "basic" => Ok(DebugLevel::Basic),
            "detailed" => Ok(DebugLevel::Detailed),
            "trace" => Ok(DebugLevel::Trace),
            _ => Err(format!("Invalid debug level: {}", s)),
        }
    }

    pub fn includes(&self, level: DebugLevel) -> bool {
        match self {
            DebugLevel::Off => false,
            DebugLevel::Basic => matches!(level, DebugLevel::Basic),
            DebugLevel::Detailed => matches!(level, DebugLevel::Basic | DebugLevel::Detailed),
            DebugLevel::Trace => true,
        }
    }
}

static DEBUG_CONFIG: OnceLock<DebugConfig> = OnceLock::new();

pub fn set_debug_level(level: DebugLevel) {
    set_debug_config(DebugConfig {
        level,
        position_filter: None,
        failure_only: false,
        trace_file: None,
        enabled_categories: None,
    });
}

pub fn set_debug_config(config: DebugConfig) {
    DEBUG_CONFIG.set(config).unwrap_or_else(|_| {
        eprintln!("Warning: Debug config already set");
    });
}

pub fn set_debug_with_trace_file(level: DebugLevel, trace_file: Option<String>) {
    set_debug_config(DebugConfig {
        level,
        position_filter: None,
        failure_only: false,
        trace_file,
        enabled_categories: None,
    });
}

pub fn get_debug_level() -> DebugLevel {
    DEBUG_CONFIG
        .get()
        .unwrap_or(&DebugConfig {
            level: DebugLevel::Off,
            position_filter: None,
            failure_only: false,
            trace_file: None,
            enabled_categories: None,
        })
        .level
}

pub fn get_debug_config() -> &'static DebugConfig {
    DEBUG_CONFIG.get().unwrap_or(&DebugConfig {
        level: DebugLevel::Off,
        position_filter: None,
        failure_only: false,
        trace_file: None,
        enabled_categories: None,
    })
}

// Helper function to write debug output to file or stdout
fn write_debug_output(msg: &str) {
    let config = get_debug_config();
    if let Some(ref trace_file) = config.trace_file {
        // Write to file
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(trace_file)
        {
            writeln!(file, "{}", msg).ok();
        } else {
            eprintln!("Warning: Could not write to trace file {}", trace_file);
            println!("{}", msg);
        }
    } else {
        // Write to stdout
        println!("{}", msg);
    }
}

// Simple function-based debug calls that are easier to export
pub fn debug_basic_print(msg: &str) {
    if get_debug_level().includes(DebugLevel::Basic) {
        println!("{}", msg);
    }
}

pub fn debug_detailed_print(msg: &str) {
    if get_debug_level().includes(DebugLevel::Detailed) {
        println!("{}", msg);
    }
}

pub fn debug_trace_print(msg: &str) {
    if get_debug_level().includes(DebugLevel::Trace) {
        println!("{}", msg);
    }
}

pub fn debug_grammar_print(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("GRAMMAR") {
        // Use structured format and write to trace file if configured
        if msg.starts_with("GRAMMAR|") {
            write_debug_output(msg);
        } else {
            write_debug_output(&format!("GRAMMAR|{}", msg));
        }
    }
}

pub fn debug_parser_print(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) {
        println!("[PARSER] {}", msg);
    }
}

pub fn debug_earley_print(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) {
        write_debug_output(&format!("EARLEY|{}", msg));
    }
}

// Helper function to check if a category is enabled
fn is_category_enabled(category: &str) -> bool {
    let config = get_debug_config();
    match &config.enabled_categories {
        None => true, // If no filter specified, all categories enabled
        Some(categories) => categories.contains(&category.to_uppercase()),
    }
}

// Category-specific debug functions
pub fn debug_bootstrap(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("BOOTSTRAP") {
        write_debug_output(&format!("BOOTSTRAP|{}", msg));
    }
}

pub fn debug_queue(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("QUEUE") {
        write_debug_output(&format!("QUEUE|{}", msg));
    }
}

pub fn debug_scanner(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("SCANNER") {
        write_debug_output(&format!("SCANNER|{}", msg));
    }
}

pub fn debug_output(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("OUTPUT") {
        write_debug_output(&format!("OUTPUT|{}", msg));
    }
}

pub fn debug_predict(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("PREDICT") {
        write_debug_output(&format!("PREDICT|{}", msg));
    }
}

pub fn debug_complete(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("COMPLETE") {
        write_debug_output(&format!("COMPLETE|{}", msg));
    }
}

pub fn debug_dedup(level: DebugLevel, msg: &str) {
    if get_debug_level().includes(level) && is_category_enabled("DEDUP") {
        write_debug_output(&format!("DEDUP|{}", msg));
    }
}

// Specialized Earley debug functions with position filtering
pub fn debug_earley_at_pos(level: DebugLevel, pos: usize, msg: &str) {
    let config = get_debug_config();
    if !config.level.includes(level) {
        return;
    }

    // Apply position filter if set
    if let Some(filter_pos) = config.position_filter {
        if pos != filter_pos {
            return;
        }
    }

    // Use structured format for easier grepping
    write_debug_output(&format!("EARLEY|S({})|{}", pos, msg));
}

pub fn debug_earley_failure(pos: usize, expected: &str, actual: char, queue_snapshot: &str) {
    let config = get_debug_config();
    if !config.level.includes(DebugLevel::Trace) {
        return;
    }

    write_debug_output(&format!(
        "EARLEY-FAIL|S({})|expected={}|actual='{}'|queue=[{}]",
        pos, expected, actual, queue_snapshot
    ));
}

// Specialized Earley operation functions for structured logging
pub fn debug_earley_completer(pos: usize, task_info: &str) {
    debug_earley_at_pos(
        DebugLevel::Trace,
        pos,
        &format!("op=COMPLETER|task={}", task_info),
    );
}

pub fn debug_earley_predictor(pos: usize, task_info: &str, mark: &str, name: &str) {
    debug_earley_at_pos(
        DebugLevel::Trace,
        pos,
        &format!(
            "op=PREDICTOR|task={}|mark={}|name={}",
            task_info, mark, name
        ),
    );
}

pub fn debug_earley_scanner(pos: usize, task_info: &str, tmark: &str, matcher: &str) {
    debug_earley_at_pos(
        DebugLevel::Trace,
        pos,
        &format!(
            "op=SCANNER|task={}|tmark={}|matcher={}",
            task_info, tmark, matcher
        ),
    );
}

pub fn debug_earley_scanner_match(pos: usize, matched_char: char, new_pos: usize) {
    debug_earley_at_pos(
        DebugLevel::Trace,
        pos,
        &format!(
            "op=SCANNER-MATCH|char='{}'|new_pos={}",
            matched_char, new_pos
        ),
    );
}

// Convenience macros for formatted printing
#[macro_export]
macro_rules! debug_basic {
    ($($arg:tt)*) => {
        $crate::debug::debug_basic_print(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_detailed {
    ($($arg:tt)*) => {
        $crate::debug::debug_detailed_print(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_trace {
    ($($arg:tt)*) => {
        $crate::debug::debug_trace_print(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_grammar {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_grammar_print($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_parser {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_parser_print($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_earley {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_earley_print($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_earley_pos {
    ($level:expr, $pos:expr, $($arg:tt)*) => {
        $crate::debug::debug_earley_at_pos($level, $pos, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_earley_fail {
    ($pos:expr, $expected:expr, $actual:expr, $queue_snapshot:expr) => {
        $crate::debug::debug_earley_failure($pos, $expected, $actual, $queue_snapshot)
    };
}

// Category-specific macros for structured debug messages
#[macro_export]
macro_rules! debug_bootstrap {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_bootstrap($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_queue {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_queue($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_scanner {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_scanner($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_output {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_output($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_predict {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_predict($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_complete {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_complete($level, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_dedup {
    ($level:expr, $($arg:tt)*) => {
        $crate::debug::debug_dedup($level, &format!($($arg)*))
    };
}

// Parse failure debugging
pub fn debug_parse_failure(input: &str, position: usize, error: &str) {
    if get_debug_level() == DebugLevel::Off {
        return;
    }

    println!("=== PARSE FAILURE ===");
    println!("Error: {}", error);
    println!("Input: {}", input);

    if position < input.len() {
        println!(
            "Failed at position {}: '{}'",
            position,
            input.chars().nth(position).unwrap_or('?')
        );

        // Show context around failure point
        let start = position.saturating_sub(10);
        let end = (position + 10).min(input.len());
        let context = &input[start..end];
        let pointer_pos = position - start;

        println!("Context: {}", context);
        println!("         {}^", " ".repeat(pointer_pos));
    } else {
        println!("Failed at end of input (position {})", position);
    }
}
