use unicode_character_database::general_category::{LETTER, NONSPACING_MARK, DECIMAL_NUMBER, SPACE_SEPARATOR};

#[derive(Clone, Copy, Debug)]
/// WIP. Immediate focus on things needed for ixml_grammar
/// See: https://en.wikipedia.org/wiki/Unicode_character_property#General_Category
/// TODO: would https://docs.rs/unicode_categories/ be better?
pub enum UnicodeRange {
    L,
    Mn,
    Nd,
    Zs,
}

impl UnicodeRange {
    pub fn new(name: &str) -> UnicodeRange {
        match name {
            "L" => UnicodeRange::L,
            "Mn" => UnicodeRange::Mn,
            "Nd" => UnicodeRange::Nd,
            "Zs" => UnicodeRange::Zs,
            _ => panic!("Referenced unknown Unicode Category {name}")
        }
    }

    pub fn accept(&self, ch: char) -> bool {
        match self {
            UnicodeRange::L => member_of_category(ch, LETTER),
            UnicodeRange::Mn => member_of_category(ch, NONSPACING_MARK),
            UnicodeRange::Nd => member_of_category(ch, DECIMAL_NUMBER),
            UnicodeRange::Zs => member_of_category(ch, SPACE_SEPARATOR),
        }
    }
}

fn member_of_category(ch: char, spec: &'static [(u32,u32)]) -> bool {
    let codepoint = ch as u32;
    spec.iter().any(|(bot, top)| *bot <= codepoint && codepoint <= *top)
}
