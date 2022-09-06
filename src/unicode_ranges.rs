use unicode_character_database::general_category::{LETTER, NONSPACING_MARK, DECIMAL_NUMBER, SPACE_SEPARATOR};

#[derive(Clone, Copy, Debug)]
/// WIP. Immediate focus on things needed for `ixml_grammar`
/// See: https://en.wikipedia.org/wiki/Unicode_character_property#General_Category
/// TODO: would https://docs.rs/unicode_categories/ be better?
pub enum UnicodeRange {
    L,
    Mn,
    Nd,
    Zs,
}

impl UnicodeRange {
    pub fn new(name: &str) -> Self {
        match name {
            "L" => Self::L,
            "Mn" => Self::Mn,
            "Nd" => Self::Nd,
            "Zs" => Self::Zs,
            _ => panic!("Referenced unknown Unicode Category {name}")
        }
    }

    pub fn accept(&self, ch: char) -> bool {
        match self {
            Self::L => member_of_category(ch, LETTER),
            Self::Mn => member_of_category(ch, NONSPACING_MARK),
            Self::Nd => member_of_category(ch, DECIMAL_NUMBER),
            Self::Zs => member_of_category(ch, SPACE_SEPARATOR),
        }
    }
}

fn member_of_category(ch: char, spec: &'static [(u32,u32)]) -> bool {
    let codepoint = ch as u32;
    spec.iter().any(|(bot, top)| *bot <= codepoint && codepoint <= *top)
}
