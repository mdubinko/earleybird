/// Project-wide small immutable string for nonterminal names and identity keys.
/// Defined in one place so the backing small-string crate can be swapped here
/// without touching call sites. The accessed surface is string-like only
/// (`from`/`new` constructors plus `Deref<str>`), so any SSO crate can stand in.
pub type EarleyStr = smol_str::SmolStr;

pub mod alloc_count;
pub mod debug;
pub mod grammar;
pub mod ixml_bootstrap;
pub mod parser;
pub mod test_grammars;
pub mod testsuite_utils;
pub mod unicode_ranges;
pub mod utils;
pub mod validator;
