//! A port of the reference parser of the password rules language,
//! `tools/PasswordRulesParser/PasswordRulesParser.js` in Apple's password-manager-resources at the
//! commit `data/SOURCES` pins: function for function, with its test cases, 166 checks. Like the
//! reference it never fails, and leaves out what it cannot read.

use crate::charset::{CharacterSet, is_ascii_printable};

/// A named character class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Named {
    AsciiPrintable,
    Digit,
    Lower,
    Special,
    Unicode,
    Upper,
}

impl Named {
    /// Each class and its identifier.
    const IDENTIFIERS: [(Self, &'static str); 6] = [
        (Self::AsciiPrintable, "ascii-printable"),
        (Self::Digit, "digit"),
        (Self::Lower, "lower"),
        (Self::Special, "special"),
        (Self::Unicode, "unicode"),
        (Self::Upper, "upper"),
    ];

    /// The class an identifier names, in any case (`UPPER` is `upper`).
    fn from_identifier(identifier: &str) -> Option<Self> {
        Self::IDENTIFIERS.iter().find(|(_, name)| name.eq_ignore_ascii_case(identifier)).map(|&(named, _)| named)
    }
}

/// One value of a `required` or `allowed` property: a named class or `[characters]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Class {
    Named(Named),
    Custom(Vec<char>),
}

/// A property as written, before canonicalization. `None` is the reference parser's `null`: a
/// property without a value it could read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Written {
    Required(Option<Vec<Class>>),
    Allowed(Option<Vec<Class>>),
    MaxConsecutive(Option<u64>),
    MaxRepeating(Option<u64>),
    MaxSequential(Option<u64>),
    MinLength(Option<u64>),
    MaxLength(Option<u64>),
}

/// A property with its value: as written, once [`parse_rules_internal`] has kept it, or in
/// canonical form, as [`canonical_rules`] (the reference's `parsePasswordRules`) returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Rule {
    Required(Vec<Class>),
    Allowed(Vec<Class>),
    MaxConsecutive(u64),
    MaxRepeating(u64),
    MaxSequential(u64),
    MinLength(u64),
    MaxLength(u64),
}

// The lexer and parser below follow the reference's functions of the same names. Positions
// index characters; the reference indexes UTF-16 units, which only differs for characters
// outside the Basic Multilingual Plane, and those are skipped by both.

fn is_identifier_character(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '-'
}

fn is_ascii_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\u{c}' | '\n' | '\r' | '\t')
}

/// `_indexOfNonWhitespaceCharacter`.
fn index_of_non_whitespace(input: &[char], mut position: usize) -> usize {
    while input.get(position).is_some_and(|&c| is_ascii_whitespace(c)) {
        position += 1;
    }
    position
}

/// `_parseIdentifier`: the identifier characters from `position` on.
fn parse_identifier(input: &[char], mut position: usize) -> (String, usize) {
    let mut identifier = String::new();
    while let Some(&c) = input.get(position).filter(|&&c| is_identifier_character(c)) {
        identifier.push(c);
        position += 1;
    }
    (identifier, position)
}

/// `_parseInteger`: digits up to the end or a `;`, anything else spoils the value. Saturates
/// where the reference loses precision (past 2^53), so huge lengths stay huge.
fn parse_integer(input: &[char], mut position: usize) -> (Option<u64>, usize) {
    if !input.get(position).is_some_and(char::is_ascii_digit) {
        return (None, position);
    }
    let mut result: u64 = 0;
    while let Some(digit) = input.get(position).and_then(|c| c.to_digit(10)) {
        result = result.saturating_mul(10).saturating_add(u64::from(digit));
        position += 1;
    }
    match input.get(position) {
        None | Some(';') => (Some(result), position),
        Some(_) => (None, position),
    }
}

/// `_parseCustomCharacterClass`: `[characters]` at `position`. Characters other than ASCII
/// printable ones are skipped, a `-` counts only first, and `]]` at the end is a literal `]`.
fn parse_custom_character_class(input: &[char], mut position: usize) -> (Option<Vec<char>>, usize) {
    let length = input.len();
    position += 1;
    if position >= length {
        return (None, position);
    }
    let initial = position;
    let mut result = Vec::new();
    loop {
        let c = input[position];
        position += 1;
        if is_ascii_printable(c) && !(c == '-' && position - 1 > initial) {
            result.push(c);
            if c == ']' {
                break;
            }
        }
        if position >= length {
            break;
        }
    }
    if (position < length && input[position] != ']') || (position == length && input[position - 1] == ']') {
        // The closing bracket went into the class: take it out.
        result.pop();
        return (Some(result), position);
    }
    if position < length && input[position] == ']' {
        return (Some(result), position + 1);
    }
    (None, position)
}

/// `_parsePasswordRequiredOrAllowedPropertyValue`: classes separated by commas, up to the end
/// or a `;`. An empty `[]` adds nothing.
fn parse_required_or_allowed_value(input: &[char], mut position: usize) -> (Option<Vec<Class>>, usize) {
    let length = input.len();
    let mut values = Vec::new();
    loop {
        let c = input[position];
        if is_identifier_character(c) {
            let start = position;
            let (identifier, next) = parse_identifier(input, position);
            position = next;
            let Some(named) = Named::from_identifier(&identifier) else { return (None, start) };
            values.push(Class::Named(named));
        } else if c == '[' {
            let (characters, next) = parse_custom_character_class(input, position);
            position = next;
            if let Some(characters) = characters.filter(|characters| !characters.is_empty()) {
                values.push(Class::Custom(characters));
            }
        } else {
            return (None, position);
        }
        position = index_of_non_whitespace(input, position);
        if position >= length || input[position] == ';' {
            break;
        }
        if input[position] == ',' {
            position = index_of_non_whitespace(input, position + 1);
            if position >= length {
                return (None, position);
            }
            continue;
        }
        return (None, position);
    }
    (Some(values), position)
}

/// `_parsePasswordRule`: one `name: value`. An unknown name, or no colon after it, is no rule.
fn parse_rule(input: &[char], position: usize) -> (Option<Written>, usize) {
    let length = input.len();
    let start = position;
    let (identifier, mut position) = parse_identifier(input, position);
    let empty = match identifier.as_str() {
        "required" => Written::Required(None),
        "allowed" => Written::Allowed(None),
        "max-consecutive" => Written::MaxConsecutive(None),
        "max-repeating" => Written::MaxRepeating(None),
        "max-sequential" => Written::MaxSequential(None),
        "minlength" => Written::MinLength(None),
        "maxlength" => Written::MaxLength(None),
        _ => return (None, start),
    };
    if position >= length || input[position] != ':' {
        return (None, position);
    }
    position = index_of_non_whitespace(input, position + 1);
    if position >= length || input[position] == ';' {
        return (Some(empty), position);
    }
    // A number counts only when it is not zero, as in the reference (zero is falsy there).
    let number = |(value, position): (Option<u64>, usize)| (value.filter(|&value| value != 0), position);
    let rule = match empty {
        Written::Required(_) => {
            let (value, next) = parse_required_or_allowed_value(input, position);
            position = next;
            Written::Required(value)
        }
        Written::Allowed(_) => {
            let (value, next) = parse_required_or_allowed_value(input, position);
            position = next;
            Written::Allowed(value)
        }
        integer => {
            let (value, next) = number(parse_integer(input, position));
            position = next;
            match integer {
                Written::MaxConsecutive(_) => Written::MaxConsecutive(value),
                Written::MaxRepeating(_) => Written::MaxRepeating(value),
                Written::MaxSequential(_) => Written::MaxSequential(value),
                Written::MinLength(_) => Written::MinLength(value),
                _ => Written::MaxLength(value),
            }
        }
    };
    (Some(rule), position)
}

impl Written {
    /// The rule with its value; `None` without one, as the reference keeps only those.
    fn valued(self) -> Option<Rule> {
        Some(match self {
            Self::Required(value) => Rule::Required(value?),
            Self::Allowed(value) => Rule::Allowed(value?),
            Self::MaxConsecutive(value) => Rule::MaxConsecutive(value?),
            Self::MaxRepeating(value) => Rule::MaxRepeating(value?),
            Self::MaxSequential(value) => Rule::MaxSequential(value?),
            Self::MinLength(value) => Rule::MinLength(value?),
            Self::MaxLength(value) => Rule::MaxLength(value?),
        })
    }
}

/// `_parsePasswordRulesInternal`: the rules with a value, in order. `None` (the reference's
/// `null`) when something other than a `;` follows a rule; a stray character where a rule
/// should start ends the list instead, keeping what came before.
fn parse_rules_internal(input: &[char]) -> Option<Vec<Rule>> {
    let length = input.len();
    let mut rules = Vec::new();
    let mut position = index_of_non_whitespace(input, 0);
    while position < length {
        if !is_identifier_character(input[position]) {
            return Some(rules);
        }
        let (rule, next) = parse_rule(input, position);
        rules.extend(rule.and_then(Written::valued));
        position = index_of_non_whitespace(input, next);
        if position >= length {
            break;
        }
        if input[position] == ';' {
            position = index_of_non_whitespace(input, position + 1);
            if position >= length {
                return Some(rules);
            }
            continue;
        }
        return None;
    }
    Some(rules)
}

/// `_canonicalizedPropertyValues`: the classes a list of values covers, as named classes where
/// a whole one is covered and one custom class for the rest (in the reference's order: upper,
/// lower and digits left over, then special characters). `unicode` or `ascii-printable`, the
/// first of them met, stands for the whole list.
fn canonicalized(values: &[Class]) -> Vec<Class> {
    let mut covered = CharacterSet::EMPTY;
    for value in values {
        match value {
            Class::Named(named @ (Named::Unicode | Named::AsciiPrintable)) => return vec![Class::Named(*named)],
            Class::Named(named) => covered = covered.union(named.set()),
            Class::Custom(characters) => covered = covered.union(CharacterSet::of(characters.iter().copied())),
        }
    }
    let mut rest = Vec::new();
    let mut whole = |named: Named| {
        let present = covered.intersection(named.set());
        let whole = present == named.set();
        if !whole {
            rest.extend(present.chars());
        }
        whole
    };
    let wholes = [Named::Upper, Named::Lower, Named::Digit, Named::Special].map(|named| (named, whole(named)));
    if wholes.iter().all(|&(_, whole)| whole) {
        return vec![Class::Named(Named::AsciiPrintable)];
    }
    let mut result: Vec<Class> = wholes.iter().filter(|&&(_, whole)| whole).map(|&(named, _)| Class::Named(named)).collect();
    if !rest.is_empty() {
        result.push(Class::Custom(rest));
    }
    result
}

/// `parsePasswordRules`: the required rules in order, then one `allowed` that also takes every
/// required class (`ascii-printable` when nothing is said), then the smallest of each
/// `max-*`, the largest `minlength` and the smallest `maxlength`.
pub(crate) fn canonical_rules(text: &str) -> Vec<Rule> {
    let input: Vec<char> = text.chars().collect();
    let mut rules = Vec::new();
    let mut allowed = Vec::new();
    let (mut max_consecutive, mut max_repeating, mut max_sequential, mut min_length, mut max_length) = (None, None, None, 0, None);
    let smallest = |held: Option<u64>, value: u64| Some(held.map_or(value, |held| held.min(value)));
    for rule in parse_rules_internal(&input).unwrap_or_default() {
        match rule {
            Rule::MaxConsecutive(value) => max_consecutive = smallest(max_consecutive, value),
            Rule::MaxRepeating(value) => max_repeating = smallest(max_repeating, value),
            Rule::MaxSequential(value) => max_sequential = smallest(max_sequential, value),
            Rule::MinLength(value) => min_length = min_length.max(value),
            Rule::MaxLength(value) => max_length = smallest(max_length, value),
            Rule::Required(values) => {
                let values = canonicalized(&values);
                allowed.extend(values.iter().cloned());
                rules.push(Rule::Required(values));
            }
            Rule::Allowed(values) => allowed.extend(values),
        }
    }
    let allowed = canonicalized(&allowed);
    rules.push(Rule::Allowed(if allowed.is_empty() { vec![Class::Named(Named::AsciiPrintable)] } else { allowed }));
    rules.extend(max_consecutive.map(Rule::MaxConsecutive));
    rules.extend(max_repeating.map(Rule::MaxRepeating));
    rules.extend(max_sequential.map(Rule::MaxSequential));
    if min_length > 0 {
        rules.push(Rule::MinLength(min_length));
    }
    rules.extend(max_length.map(Rule::MaxLength));
    rules
}

impl Class {
    /// The characters a list of classes covers together.
    pub(crate) fn set_of(classes: &[Class]) -> CharacterSet {
        classes.iter().fold(CharacterSet::EMPTY, |set, class| {
            set.union(match class {
                Class::Named(named) => named.set(),
                Class::Custom(characters) => CharacterSet::of(characters.iter().copied()),
            })
        })
    }
}

impl Named {
    fn set(self) -> CharacterSet {
        match self {
            Self::AsciiPrintable => CharacterSet::ASCII_PRINTABLE,
            Self::Digit => CharacterSet::DIGIT,
            Self::Lower => CharacterSet::LOWER,
            Self::Special => CharacterSet::SPECIAL,
            Self::Unicode => CharacterSet::UNICODE,
            Self::Upper => CharacterSet::UPPER,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    /// A class as the reference's tests write it: its name, or `[characters]`.
    fn describe_class(class: &Class) -> Value {
        match class {
            Class::Named(named) => Value::from(Named::IDENTIFIERS.iter().find(|(class, _)| class == named).unwrap().1),
            Class::Custom(characters) => Value::from(format!("[{}]", characters.iter().collect::<String>())),
        }
    }

    fn describe_classes(classes: &[Class]) -> Value {
        Value::Array(classes.iter().map(describe_class).collect())
    }

    fn describe_written(rule: &Written) -> Value {
        let classes = |value: &Option<Vec<Class>>| value.as_deref().map(describe_classes).unwrap_or(Value::Null);
        let number = |value: &Option<u64>| value.map(Value::from).unwrap_or(Value::Null);
        let (name, value) = match rule {
            Written::Required(value) => ("required", classes(value)),
            Written::Allowed(value) => ("allowed", classes(value)),
            Written::MaxConsecutive(value) => ("max-consecutive", number(value)),
            Written::MaxRepeating(value) => ("max-repeating", number(value)),
            Written::MaxSequential(value) => ("max-sequential", number(value)),
            Written::MinLength(value) => ("minlength", number(value)),
            Written::MaxLength(value) => ("maxlength", number(value)),
        };
        json!({ "name": name, "value": value })
    }

    fn describe_rules(rules: &[Rule]) -> Value {
        Value::Array(
            rules
                .iter()
                .map(|rule| {
                    let (name, value) = match rule {
                        Rule::Required(classes) => ("required", describe_classes(classes)),
                        Rule::Allowed(classes) => ("allowed", describe_classes(classes)),
                        Rule::MaxConsecutive(value) => ("max-consecutive", Value::from(*value)),
                        Rule::MaxRepeating(value) => ("max-repeating", Value::from(*value)),
                        Rule::MaxSequential(value) => ("max-sequential", Value::from(*value)),
                        Rule::MinLength(value) => ("minlength", Value::from(*value)),
                        Rule::MaxLength(value) => ("maxlength", Value::from(*value)),
                    };
                    json!({ "name": name, "value": value })
                })
                .collect(),
        )
    }

    // Ported from tools/PasswordRulesParser/PasswordRulesParserTest.js at the pinned commit
    // (4fba5d1baf34dff9b2ff4186d1fcc7ad8bb00af9), case for case: 166 checks in nine tables.

    #[test]
    fn upstream_skipping_whitespace() {
        let cases: [(&str, usize, &str); 11] = [
            ("", 0, ""),
            (" ", 0, ""),
            ("a", 0, "a"),
            ("  ", 0, ""),
            ("  a", 0, "a"),
            ("\t  \t", 0, ""),
            ("\n\n\ta", 0, "a"),
            ("\r\na", 0, "a"),
            ("\r\na", 1, "a"),
            ("\r\na", 2, "a"),
            ("\r\na", 3, ""),
        ];
        for (input, offset, expected) in cases {
            let input: Vec<char> = input.chars().collect();
            let index = index_of_non_whitespace(&input, offset);
            let landed_on: String = input.get(index).map(char::to_string).unwrap_or_default();
            assert_eq!(landed_on, expected, "{input:?} from {offset}");
        }
    }

    #[test]
    fn upstream_parsing_integer() {
        let cases: [(&str, usize, Option<u64>); 10] = [
            ("a", 0, None),
            ("0", 0, Some(0)),
            ("12", 0, Some(12)),
            ("127", 0, Some(127)),
            ("1a", 0, None),
            ("12a", 0, None),
            ("1;", 0, Some(1)),
            ("12;", 1, Some(2)),
            ("abc1234;", 3, Some(1234)),
            ("9007199254740991", 0, Some(9007199254740991)),
        ];
        for (input, offset, expected) in cases {
            let chars: Vec<char> = input.chars().collect();
            assert_eq!(parse_integer(&chars, offset).0, expected, "{input:?} from {offset}");
        }
    }

    #[test]
    fn upstream_parsing_custom_character_class() {
        let cases: [(&str, usize, Option<&str>); 28] = [
            ("[]", 0, Some("")),
            ("[ ]", 0, Some(" ")),
            ("[  ]", 0, Some("  ")),
            ("[a]", 0, Some("a")),
            ("[ab]", 0, Some("ab")),
            ("[abc]", 0, Some("abc")),
            ("[abc01A]", 0, Some("abc01A")),
            ("[12]", 0, Some("12")),
            ("[bab]", 0, Some("bab")),
            ("[abc];", 0, Some("abc")),
            ("[-]", 0, Some("-")),
            ("[a-]", 0, Some("a")),
            ("[a-b]", 0, Some("ab")),
            ("[-a]", 0, Some("-a")),
            ("[-ab]", 0, Some("-ab")),
            ("abcdefg[-ab];", 7, Some("-ab")),
            ("[]]", 0, Some("]")),
            ("[a]]", 0, Some("a]")),
            ("[abc]]", 0, Some("abc]")),
            ("[-a]]", 0, Some("-a]")),
            ("[", 0, None),
            ("[a", 0, None),
            ("[]a]", 0, Some("")),
            ("[a]b]", 0, Some("a")),
            ("[☃]", 0, Some("")),
            ("[a☃]", 0, Some("a")),
            ("[☃a]", 0, Some("a")),
            ("[a☃b]", 0, Some("ab")),
        ];
        for (input, offset, expected) in cases {
            let chars: Vec<char> = input.chars().collect();
            let parsed = parse_custom_character_class(&chars, offset).0.map(|characters| characters.into_iter().collect::<String>());
            assert_eq!(parsed.as_deref(), expected, "{input:?} from {offset}");
        }
    }

    #[test]
    fn upstream_parsing_required_or_allowed_value() {
        let cases: [(&str, &str); 29] = [
            ("ascii-printable", r#"["ascii-printable"]"#),
            ("digit", r#"["digit"]"#),
            ("lower", r#"["lower"]"#),
            ("upper", r#"["upper"]"#),
            ("special", r#"["special"]"#),
            ("unicode", r#"["unicode"]"#),
            ("UPPER", r#"["upper"]"#),
            ("UPpER", r#"["upper"]"#),
            ("upper, lower", r#"["upper","lower"]"#),
            ("upper,  lower", r#"["upper","lower"]"#),
            ("upper,  \tlower", r#"["upper","lower"]"#),
            ("[a]", r#"["[a]"]"#),
            ("[a], upper", r#"["[a]","upper"]"#),
            ("upper, [a]", r#"["upper","[a]"]"#),
            ("upper, [a], lower", r#"["upper","[a]","lower"]"#),
            ("[a], [b]", r#"["[a]","[b]"]"#),
            ("[], [b]", r#"["[b]"]"#),
            ("[a]], [b]", r#"["[a]]","[b]"]"#),
            ("[a], lower, [b]", r#"["[a]","lower","[b]"]"#),
            ("lower;", r#"["lower"]"#),
            ("lower ;", r#"["lower"]"#),
            ("lower \t;", r#"["lower"]"#),
            (" ", r#"null"#),
            ("☃", r#"null"#),
            ("dummy", r#"null"#),
            ("upper, dummy", r#"null"#),
            ("upper,", r#"null"#),
            (";", r#"null"#),
            ("upper d", r#"null"#),
        ];
        for (input, expected) in cases {
            let chars: Vec<char> = input.chars().collect();
            let parsed = parse_required_or_allowed_value(&chars, 0).0;
            assert_eq!(parsed.as_deref().map(describe_classes).unwrap_or(Value::Null), json(expected), "{input:?}");
        }
    }

    #[test]
    fn upstream_parsing_single_rule() {
        let cases: [(&str, &str); 24] = [
            ("required:", r#"{"name":"required","value":null}"#),
            ("required: ", r#"{"name":"required","value":null}"#),
            ("required:;", r#"{"name":"required","value":null}"#),
            ("required: ;", r#"{"name":"required","value":null}"#),
            ("allowed:", r#"{"name":"allowed","value":null}"#),
            ("allowed: ;", r#"{"name":"allowed","value":null}"#),
            ("max-consecutive:", r#"{"name":"max-consecutive","value":null}"#),
            ("max-consecutive: ;", r#"{"name":"max-consecutive","value":null}"#),
            ("max-repeating:", r#"{"name":"max-repeating","value":null}"#),
            ("max-repeating: ;", r#"{"name":"max-repeating","value":null}"#),
            ("max-sequential:", r#"{"name":"max-sequential","value":null}"#),
            ("max-sequential: ;", r#"{"name":"max-sequential","value":null}"#),
            ("required: upper", r#"{"name":"required","value":["upper"]}"#),
            ("required: upper, lower", r#"{"name":"required","value":["upper","lower"]}"#),
            ("required: upper, [*], lower", r#"{"name":"required","value":["upper","[*]","lower"]}"#),
            ("allowed: upper", r#"{"name":"allowed","value":["upper"]}"#),
            ("allowed: upper, [*], lower", r#"{"name":"allowed","value":["upper","[*]","lower"]}"#),
            ("max-consecutive:2", r#"{"name":"max-consecutive","value":2}"#),
            ("max-repeating:2", r#"{"name":"max-repeating","value":2}"#),
            ("max-sequential:2", r#"{"name":"max-sequential","value":2}"#),
            ("minlength: 12;", r#"{"name":"minlength","value":12}"#),
            ("maxlength: 73", r#"{"name":"maxlength","value":73}"#),
            ("dummy", r#"null"#),
            ("required", r#"null"#),
        ];
        for (input, expected) in cases {
            let chars: Vec<char> = input.chars().collect();
            let parsed = parse_rule(&chars, 0).0;
            assert_eq!(parsed.as_ref().map(describe_written).unwrap_or(Value::Null), json(expected), "{input:?}");
        }
    }

    #[test]
    fn upstream_parsing_rules_internal() {
        let cases: [(&str, &str); 18] = [
            ("required: upper", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper;", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper; ", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper;;", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper; ;", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper;;;", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper ", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper ; ", r#"[{"name":"required","value":["upper"]}]"#),
            ("required: upper; required: lower", r#"[{"name":"required","value":["upper"]},{"name":"required","value":["lower"]}]"#),
            ("required: upper; allowed: [ab]", r#"[{"name":"required","value":["upper"]},{"name":"allowed","value":["[ab]"]}]"#),
            ("allowed: upper; allowed: lower", r#"[{"name":"allowed","value":["upper"]},{"name":"allowed","value":["lower"]}]"#),
            (
                "max-consecutive: 2; max-repeating: 3; max-sequential: 4",
                r#"[{"name":"max-consecutive","value":2},{"name":"max-repeating","value":3},{"name":"max-sequential","value":4}]"#,
            ),
            ("minlength: 12", r#"[{"name":"minlength","value":12}]"#),
            ("minlength:        12", r#"[{"name":"minlength","value":12}]"#),
            ("minlength: 12; maxlength: 73", r#"[{"name":"minlength","value":12},{"name":"maxlength","value":73}]"#),
            ("required: ;", r#"[]"#),
            ("☃", r#"[]"#),
            ("required: upper d", r#"null"#),
        ];
        for (input, expected) in cases {
            let chars: Vec<char> = input.chars().collect();
            let parsed = parse_rules_internal(&chars).map(|rules| describe_rules(&rules));
            assert_eq!(parsed.unwrap_or(Value::Null), json(expected), "{input:?}");
        }
    }

    #[test]
    fn upstream_canonicalizing_character_classes() {
        let named = |name: &str| Class::Named(Named::from_identifier(name).unwrap());
        let custom = |characters: &str| Class::Custom(characters.chars().collect());
        let cases: [(&str, Vec<Class>, &str); 12] = [
            ("single named class", vec![named("upper")], r#"["upper"]"#),
            ("duplicate named class", vec![named("upper"), named("upper")], r#"["upper"]"#),
            ("named + custom", vec![named("upper"), custom("abc")], r#"["upper","[abc]"]"#),
            ("custom spanning all uppercase collapses to named", vec![named("upper"), custom("ABCDEFGHIJKLMNOPQRSTUVWXYZ")], r#"["upper"]"#),
            ("custom-then-named uppercase", vec![custom("ABCDEFGHIJKLMNOPQRSTUVWXYZ"), named("upper")], r#"["upper"]"#),
            ("scattered full uppercase collapses", vec![custom("WABCXYZDEFGHIJKLMNOPQRSTUV")], r#"["upper"]"#),
            ("custom upper + custom lower", vec![custom("ABCDEFGHIJKLMNOPQRSTUVWXYZ"), custom("abcdefghijklmnopqrstuvwxyz")], r#"["upper","lower"]"#),
            ("custom special + named special", vec![custom(" !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"), named("special")], r#"["special"]"#),
            (
                "custom ascii-printable + named",
                vec![custom(" !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~"), named("ascii-printable")],
                r#"["ascii-printable"]"#,
            ),
            ("custom digits + named digit", vec![custom("0123456789"), named("digit")], r#"["digit"]"#),
            (
                "all four ranges collapse to ascii-printable",
                vec![
                    custom("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
                    custom("0123456789"),
                    custom("abcdefghijklmnopqrstuvwxyz"),
                    custom(" !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
                ],
                r#"["ascii-printable"]"#,
            ),
            ("unicode passes through", vec![named("unicode")], r#"["unicode"]"#),
        ];
        for (name, input, expected) in cases {
            assert_eq!(describe_classes(&canonicalized(&input)), json(expected), "{name}");
        }
    }

    #[test]
    fn upstream_parsing_password_rules() {
        let cases: [(&str, &str, &str); 23] = [
            ("empty input yields default allowed", "", r#"[{"name":"allowed","value":["ascii-printable"]}]"#),
            (
                "leading whitespace and single required",
                "    required: upper",
                r#"[{"name":"required","value":["upper"]},{"name":"allowed","value":["upper"]}]"#,
            ),
            ("property values are case-insensitive", "required: uPPeR", r#"[{"name":"required","value":["upper"]},{"name":"allowed","value":["upper"]}]"#),
            ("rule names are case-sensitive; a mixed-case name is unrecognized", "maxLENGTH: 7", r#"[{"name":"allowed","value":["ascii-printable"]}]"#),
            ("allowed without required does not synthesize required", "allowed:upper", r#"[{"name":"allowed","value":["upper"]}]"#),
            (
                "required plus allowed classes merge into one allowed",
                "required: upper; allowed: upper; allowed: lower",
                r#"[{"name":"required","value":["upper"]},{"name":"allowed","value":["upper","lower"]}]"#,
            ),
            (
                "duplicate max-consecutive keeps the minimum",
                "max-consecutive: 5; max-consecutive: 3",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-consecutive","value":3}]"#,
            ),
            (
                "three max-consecutive keep the minimum",
                "max-consecutive: 3; max-consecutive: 1; max-consecutive: 5",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-consecutive","value":1}]"#,
            ),
            ("single max-sequential", "max-sequential:5", r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-sequential","value":5}]"#),
            (
                "three max-sequential keep the minimum",
                "max-sequential: 3; max-sequential: 1; max-sequential: 5",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-sequential","value":1}]"#,
            ),
            ("single max-repeating", "max-repeating:5", r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-repeating","value":5}]"#),
            (
                "three max-repeating keep the minimum",
                "max-repeating: 5; max-repeating: 3; max-repeating: 1",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-repeating","value":1}]"#,
            ),
            (
                "max-consecutive and max-sequential coexist",
                "max-consecutive: 2; max-sequential: 4",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-consecutive","value":2},{"name":"max-sequential","value":4}]"#,
            ),
            (
                "all three run-length rules coexist independently",
                "max-consecutive: 1; max-repeating: 3; max-sequential: 2",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"max-consecutive","value":1},{"name":"max-repeating","value":3},{"name":"max-sequential","value":2}]"#,
            ),
            (
                "required ascii-printable plus deduped max-consecutive",
                "required: ascii-printable; max-consecutive: 5; max-consecutive: 3",
                r#"[{"name":"required","value":["ascii-printable"]},{"name":"allowed","value":["ascii-printable"]},{"name":"max-consecutive","value":3}]"#,
            ),
            (
                "custom class is canonicalized (special characters sorted)",
                "required: [*&^]; allowed: upper",
                r#"[{"name":"required","value":["[&*^]"]},{"name":"allowed","value":["upper","[&*^]"]}]"#,
            ),
            (
                "unicode required short-circuits allowed",
                "required: unicode; required: digit",
                r#"[{"name":"required","value":["unicode"]},{"name":"required","value":["digit"]},{"name":"allowed","value":["unicode"]}]"#,
            ),
            ("empty required is dropped", "required: ; required: upper", r#"[{"name":"required","value":["upper"]},{"name":"allowed","value":["upper"]}]"#),
            ("custom class of only non-ASCII becomes default allowed", "allowed: [供应商责任进展]", r#"[{"name":"allowed","value":["ascii-printable"]}]"#),
            ("ASCII characters survive among non-ASCII in a custom class", "allowed: [供应A商B责任C进展]", r#"[{"name":"allowed","value":["[ABC]"]}]"#),
            (
                "minlength keeps the maximum",
                "minlength: 12; minlength: 7; minlength: 23",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"minlength","value":23}]"#,
            ),
            (
                "maxlength keeps the minimum",
                "maxlength: 10; maxlength: 7; maxlength: 12",
                r#"[{"name":"allowed","value":["ascii-printable"]},{"name":"maxlength","value":7}]"#,
            ),
            (
                "full rule string with min and max length",
                "required: upper; allowed: upper; allowed: lower; minlength: 12; maxlength: 73;",
                r#"[{"name":"required","value":["upper"]},{"name":"allowed","value":["upper","lower"]},{"name":"minlength","value":12},{"name":"maxlength","value":73}]"#,
            ),
        ];
        for (name, input, expected) in cases {
            assert_eq!(describe_rules(&canonical_rules(input)), json(expected), "{name}");
        }
        // No error channel: malformed input degrades to the default "allowed: ascii-printable".
        let malformed: [&str; 11] = [
            "allowed: upper,,",
            "allowed: upper,;",
            "allowed: upper [a]",
            "dummy: upper",
            "upper: lower",
            "max-consecutive: [ABC]",
            "max-consecutive: upper",
            "max-consecutive: 1+1",
            "required: 1",
            "required: A",
            "allowed: 1+1",
        ];
        for input in malformed {
            assert_eq!(describe_rules(&canonical_rules(input)), json(r#"[{"name":"allowed","value":["ascii-printable"]}]"#), "{input:?}");
        }
    }
}
