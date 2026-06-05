use unicode_character_database::general_category::{
    // Letter subcategories
    CASED_LETTER,
    CLOSE_PUNCTUATION,
    // Punctuation subcategories
    CONNECTOR_PUNCTUATION,
    // Control/Other subcategories
    CONTROL,
    // Symbol subcategories
    CURRENCY_SYMBOL,
    DASH_PUNCTUATION,
    // Number subcategories
    DECIMAL_NUMBER,
    ENCLOSING_MARK,
    FINAL_PUNCTUATION,
    FORMAT,
    INITIAL_PUNCTUATION,
    LETTER,
    LETTER_NUMBER,
    // Separator subcategories
    LINE_SEPARATOR,
    LOWERCASE_LETTER,
    MARK,
    MATH_SYMBOL,
    MODIFIER_LETTER,
    MODIFIER_SYMBOL,
    NONSPACING_MARK,
    NUMBER,
    OPEN_PUNCTUATION,
    // Major categories
    OTHER,
    OTHER_LETTER,
    OTHER_NUMBER,
    OTHER_PUNCTUATION,
    OTHER_SYMBOL,
    PARAGRAPH_SEPARATOR,
    PRIVATE_USE,
    PUNCTUATION,
    SEPARATOR,
    SPACE_SEPARATOR,
    // Mark subcategories
    SPACING_MARK,
    SYMBOL,
    TITLECASE_LETTER,
    UNASSIGNED,
    UPPERCASE_LETTER,
};

#[derive(Clone, Copy, Debug)]
/// Complete implementation of Unicode General Categories for iXML 1.0 spec
/// See: https://en.wikipedia.org/wiki/Unicode_character_property#General_Category
/// See: https://invisiblexml.org/1.0/#class
pub enum UnicodeRange {
    // Major categories (single letter)
    C, // Other
    L, // Letter
    M, // Mark
    N, // Number
    P, // Punctuation
    S, // Symbol
    Z, // Separator

    // Letter subcategories
    LC, // Cased Letter
    Ll, // Lowercase Letter
    Lm, // Modifier Letter
    Lo, // Other Letter
    Lt, // Titlecase Letter
    Lu, // Uppercase Letter

    // Mark subcategories
    Mc, // Spacing Mark
    Me, // Enclosing Mark
    Mn, // Nonspacing Mark

    // Number subcategories
    Nd, // Decimal Number
    Nl, // Letter Number
    No, // Other Number

    // Punctuation subcategories
    Pc, // Connector Punctuation
    Pd, // Dash Punctuation
    Pe, // Close Punctuation
    Pf, // Final Punctuation
    Pi, // Initial Punctuation
    Po, // Other Punctuation
    Ps, // Open Punctuation

    // Symbol subcategories
    Sc, // Currency Symbol
    Sk, // Modifier Symbol
    Sm, // Math Symbol
    So, // Other Symbol

    // Separator subcategories
    Zl, // Line Separator
    Zp, // Paragraph Separator
    Zs, // Space Separator

    // Control/Other subcategories
    Cc, // Control
    Cf, // Format
    Cn, // Unassigned
    Co, // Private Use
    Cs, // Surrogate (not supported - excluded from unicode_character_database)
}

impl UnicodeRange {
    pub fn is_valid(name: &str) -> bool {
        matches!(
            name,
            "C" | "L" | "M" | "N" | "P" | "S" | "Z"
                | "LC" | "Ll" | "Lm" | "Lo" | "Lt" | "Lu"
                | "Mc" | "Me" | "Mn"
                | "Nd" | "Nl" | "No"
                | "Pc" | "Pd" | "Pe" | "Pf" | "Pi" | "Po" | "Ps"
                | "Sc" | "Sk" | "Sm" | "So"
                | "Zl" | "Zp" | "Zs"
                | "Cc" | "Cf" | "Cn" | "Co" | "Cs"
        )
    }

    pub fn new(name: &str) -> Self {
        match name {
            // Major categories
            "C" => Self::C,
            "L" => Self::L,
            "M" => Self::M,
            "N" => Self::N,
            "P" => Self::P,
            "S" => Self::S,
            "Z" => Self::Z,

            // Letter subcategories
            "LC" => Self::LC,
            "Ll" => Self::Ll,
            "Lm" => Self::Lm,
            "Lo" => Self::Lo,
            "Lt" => Self::Lt,
            "Lu" => Self::Lu,

            // Mark subcategories
            "Mc" => Self::Mc,
            "Me" => Self::Me,
            "Mn" => Self::Mn,

            // Number subcategories
            "Nd" => Self::Nd,
            "Nl" => Self::Nl,
            "No" => Self::No,

            // Punctuation subcategories
            "Pc" => Self::Pc,
            "Pd" => Self::Pd,
            "Pe" => Self::Pe,
            "Pf" => Self::Pf,
            "Pi" => Self::Pi,
            "Po" => Self::Po,
            "Ps" => Self::Ps,

            // Symbol subcategories
            "Sc" => Self::Sc,
            "Sk" => Self::Sk,
            "Sm" => Self::Sm,
            "So" => Self::So,

            // Separator subcategories
            "Zl" => Self::Zl,
            "Zp" => Self::Zp,
            "Zs" => Self::Zs,

            // Control/Other subcategories
            "Cc" => Self::Cc,
            "Cf" => Self::Cf,
            "Cn" => Self::Cn,
            "Co" => Self::Co,
            "Cs" => Self::Cs,

            _ => panic!("Referenced unknown Unicode Category {name}"),
        }
    }

    pub fn accept(&self, ch: char) -> bool {
        match self {
            // Major categories
            Self::C => member_of_category(ch, OTHER),
            Self::L => member_of_category(ch, LETTER),
            Self::M => member_of_category(ch, MARK),
            Self::N => member_of_category(ch, NUMBER),
            Self::P => member_of_category(ch, PUNCTUATION),
            Self::S => member_of_category(ch, SYMBOL),
            Self::Z => member_of_category(ch, SEPARATOR),

            // Letter subcategories
            Self::LC => member_of_category(ch, CASED_LETTER),
            Self::Ll => member_of_category(ch, LOWERCASE_LETTER),
            Self::Lm => member_of_category(ch, MODIFIER_LETTER),
            Self::Lo => member_of_category(ch, OTHER_LETTER),
            Self::Lt => member_of_category(ch, TITLECASE_LETTER),
            Self::Lu => member_of_category(ch, UPPERCASE_LETTER),

            // Mark subcategories
            Self::Mc => member_of_category(ch, SPACING_MARK),
            Self::Me => member_of_category(ch, ENCLOSING_MARK),
            Self::Mn => member_of_category(ch, NONSPACING_MARK),

            // Number subcategories
            Self::Nd => member_of_category(ch, DECIMAL_NUMBER),
            Self::Nl => member_of_category(ch, LETTER_NUMBER),
            Self::No => member_of_category(ch, OTHER_NUMBER),

            // Punctuation subcategories
            Self::Pc => member_of_category(ch, CONNECTOR_PUNCTUATION),
            Self::Pd => member_of_category(ch, DASH_PUNCTUATION),
            Self::Pe => member_of_category(ch, CLOSE_PUNCTUATION),
            Self::Pf => member_of_category(ch, FINAL_PUNCTUATION),
            Self::Pi => member_of_category(ch, INITIAL_PUNCTUATION),
            Self::Po => member_of_category(ch, OTHER_PUNCTUATION),
            Self::Ps => member_of_category(ch, OPEN_PUNCTUATION),

            // Symbol subcategories
            Self::Sc => member_of_category(ch, CURRENCY_SYMBOL),
            Self::Sk => member_of_category(ch, MODIFIER_SYMBOL),
            Self::Sm => member_of_category(ch, MATH_SYMBOL),
            Self::So => member_of_category(ch, OTHER_SYMBOL),

            // Separator subcategories
            Self::Zl => member_of_category(ch, LINE_SEPARATOR),
            Self::Zp => member_of_category(ch, PARAGRAPH_SEPARATOR),
            Self::Zs => member_of_category(ch, SPACE_SEPARATOR),

            // Control/Other subcategories
            Self::Cc => member_of_category(ch, CONTROL),
            Self::Cf => member_of_category(ch, FORMAT),
            Self::Cn => member_of_category(ch, UNASSIGNED),
            Self::Co => member_of_category(ch, PRIVATE_USE),
            Self::Cs => false, // Surrogate category excluded from unicode_character_database
        }
    }
}

fn member_of_category(ch: char, spec: &'static [(u32, u32)]) -> bool {
    let codepoint = ch as u32;
    spec.iter()
        .any(|(bot, top)| *bot <= codepoint && codepoint <= *top)
}
