use std::sync::OnceLock;
use std::fs::OpenOptions;
use std::io::Write;

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
    });
}

pub fn get_debug_level() -> DebugLevel {
    DEBUG_CONFIG.get().unwrap_or(&DebugConfig {
        level: DebugLevel::Off,
        position_filter: None,
        failure_only: false,
        trace_file: None,
    }).level
}

pub fn get_debug_config() -> &'static DebugConfig {
    DEBUG_CONFIG.get().unwrap_or(&DebugConfig {
        level: DebugLevel::Off,
        position_filter: None,
        failure_only: false,
        trace_file: None,
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
    if get_debug_level().includes(level) {
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
    write_debug_output(&format!("EARLEY|pos={}|{}", pos, msg));
}

pub fn debug_earley_failure(pos: usize, expected: &str, actual: char) {
    let config = get_debug_config();
    if !config.level.includes(DebugLevel::Trace) {
        return;
    }
    
    write_debug_output(&format!("EARLEY-FAIL|pos={}|expected={}|actual='{}'", pos, expected, actual));
}

// Specialized Earley operation functions for structured logging
pub fn debug_earley_completer(pos: usize, task_info: &str) {
    debug_earley_at_pos(DebugLevel::Trace, pos, &format!("op=COMPLETER|task={}", task_info));
}

pub fn debug_earley_predictor(pos: usize, task_info: &str, mark: &str, name: &str) {
    debug_earley_at_pos(DebugLevel::Trace, pos, &format!("op=PREDICTOR|task={}|mark={}|name={}", task_info, mark, name));
}

pub fn debug_earley_scanner(pos: usize, task_info: &str, tmark: &str, matcher: &str) {
    debug_earley_at_pos(DebugLevel::Trace, pos, &format!("op=SCANNER|task={}|tmark={}|matcher={}", task_info, tmark, matcher));
}

pub fn debug_earley_scanner_match(pos: usize, matched_char: char, new_pos: usize) {
    debug_earley_at_pos(DebugLevel::Trace, pos, &format!("op=SCANNER-MATCH|char='{}'|new_pos={}", matched_char, new_pos));
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
    ($pos:expr, $expected:expr, $actual:expr) => {
        $crate::debug::debug_earley_failure($pos, $expected, $actual)
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
        println!("Failed at position {}: '{}'", position, 
                 input.chars().nth(position).unwrap_or('?'));
        
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