//! Spoken English numbers written the way people type them: "twenty five dollars" is "$25", "nine
//! one two three" is "9123" and "at five thirty" is "at 5:30". One to nine on their own stay words,
//! as in "one of them" or "two kids", unless they're money, a percent, a time or a decimal.

const ONES: [&str; 10] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
];
const TEENS: [&str; 10] = [
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 8] = [
    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(&str, u64); 3] = [
    ("thousand", 1_000),
    ("million", 1_000_000),
    ("billion", 1_000_000_000),
];
const ONES_ORDINAL: [&str; 9] = [
    "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
];
const BIG_ORDINAL: [(&str, u64); 18] = [
    ("tenth", 10),
    ("eleventh", 11),
    ("twelfth", 12),
    ("thirteenth", 13),
    ("fourteenth", 14),
    ("fifteenth", 15),
    ("sixteenth", 16),
    ("seventeenth", 17),
    ("eighteenth", 18),
    ("nineteenth", 19),
    ("twentieth", 20),
    ("thirtieth", 30),
    ("fortieth", 40),
    ("fiftieth", 50),
    ("sixtieth", 60),
    ("seventieth", 70),
    ("eightieth", 80),
    ("ninetieth", 90),
];
/// Words before a time said without am or pm, like "at five thirty".
const TIME_WORDS: [&str; 10] = [
    "at", "by", "around", "until", "till", "before", "after", "from", "it's", "its",
];

fn ones(word: &str) -> Option<u64> {
    ONES.iter().position(|w| *w == word).map(|i| i as u64)
}

fn teens(word: &str) -> Option<u64> {
    TEENS.iter().position(|w| *w == word).map(|i| 10 + i as u64)
}

fn tens(word: &str) -> Option<u64> {
    TENS.iter()
        .position(|w| *w == word)
        .map(|i| 20 + 10 * i as u64)
}

fn scale(word: &str) -> Option<u64> {
    SCALES.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

fn ones_ordinal(word: &str) -> Option<u64> {
    ONES_ORDINAL
        .iter()
        .position(|w| *w == word)
        .map(|i| 1 + i as u64)
}

fn big_ordinal(word: &str) -> Option<u64> {
    BIG_ORDINAL
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, v)| *v)
}

fn number_word(word: &str) -> bool {
    ones(word).is_some()
        || teens(word).is_some()
        || tens(word).is_some()
        || word == "hundred"
        || scale(word).is_some()
        || ones_ordinal(word).is_some()
        || big_ordinal(word).is_some()
}

/// A word as written, split from the punctuation around it, and the space after it.
#[derive(Clone)]
struct Token {
    lead: String,
    core: String,
    trail: String,
    space: String,
}

impl Token {
    fn new(lead: &str, core: &str, trail: &str, space: &str) -> Self {
        Token {
            lead: lead.into(),
            core: core.into(),
            trail: trail.into(),
            space: space.into(),
        }
    }

    fn word(&self) -> String {
        self.core.to_lowercase().replace('\u{2019}', "'")
    }
}

fn tokens(text: &str) -> Vec<Token> {
    let start = text.len() - text.trim_start().len();
    let mut out = Vec::new();
    let mut rest = &text[start..];
    while !rest.is_empty() {
        let (piece, after) = rest.split_at(rest.find(char::is_whitespace).unwrap_or(rest.len()));
        let gap = after
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(after.len());
        let (space, next) = after.split_at(gap);
        let core_start = piece.find(char::is_alphanumeric).unwrap_or(piece.len());
        let core_end = piece
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_alphanumeric())
            .map_or(core_start, |(i, c)| i + c.len_utf8());
        out.push(Token::new(
            &piece[..core_start],
            &piece[core_start..core_end],
            &piece[core_end..],
            space,
        ));
        rest = next;
    }
    if let Some(first) = out.first_mut() {
        first.lead.insert_str(0, &text[..start]);
    }
    out
}

/// "twenty-five" is two words to the parser. Joined to other words, as in
/// "twenty-five-year-old", the number part is written in digits when it's 10 or more.
fn split_hyphens(tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::new();
    for t in tokens {
        let parts: Vec<&str> = t.core.split('-').collect();
        if parts.len() < 2 || !number_word(&parts[0].to_lowercase()) {
            out.push(t);
            continue;
        }
        let numbers = parts
            .iter()
            .take_while(|part| number_word(&part.to_lowercase()))
            .count();
        if numbers == parts.len() {
            for (k, part) in parts.iter().enumerate() {
                let last = k == parts.len() - 1;
                out.push(Token::new(
                    if k == 0 { &t.lead } else { "" },
                    part,
                    if last { &t.trail } else { "" },
                    if last { &t.space } else { "-" },
                ));
            }
            continue;
        }
        let words: Vec<Token> = parts[..numbers]
            .iter()
            .map(|part| Token::new("", part, "", " "))
            .collect();
        match parse(&words, 0) {
            Some((used, number)) if used == numbers && number.value >= 10 && !number.ordinal => {
                let core = format!("{}-{}", number.value, parts[numbers..].join("-"));
                out.push(Token::new(&t.lead, &core, &t.trail, &t.space));
            }
            _ => out.push(t),
        }
    }
    out
}

/// Whether tokens[i] follows tokens[i - 1] with no punctuation between them.
fn joined(tokens: &[Token], i: usize) -> bool {
    i > 0 && i < tokens.len() && tokens[i - 1].trail.is_empty() && tokens[i].lead.is_empty()
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// One word from zero to nine.
    Digit,
    /// 10 to 99 said plainly, which can be half of a year or a time, like "nineteen" in "nineteen
    /// ninety".
    Pair,
    Number,
}

struct Number {
    value: u64,
    kind: Kind,
    ordinal: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Last {
    Start,
    A,
    Ones,
    Teens,
    Tens,
    Hundred,
    Scale,
    And,
    Ordinal,
}

/// One number said in words from tokens[i]: how many tokens it takes and what it is.
fn parse(tokens: &[Token], i: usize) -> Option<(usize, Number)> {
    use Last::*;
    let (mut total, mut group, mut ordinal, mut last) = (0u64, 0u64, false, Start);
    let mut j = i;
    while j < tokens.len() {
        if j > i && !joined(tokens, j) {
            break;
        }
        let w = tokens[j].word();
        let next = joined(tokens, j + 1).then(|| tokens[j + 1].word());
        let next = next.as_deref();
        if last == Start && w == "a" && matches!(next, Some("hundred" | "thousand")) {
            group = 1;
            last = A;
        } else if let (Some(v), Start | Tens | Hundred | Scale | And) = (ones(&w), last) {
            group += v;
            last = Ones;
        } else if let (Some(v), Start | Hundred | Scale | And) = (teens(&w), last) {
            group += v;
            last = Teens;
        } else if let (Some(v), Start | Hundred | Scale | And) = (tens(&w), last) {
            group += v;
            last = Tens;
        } else if w == "hundred" && matches!(last, A | Ones | Teens) && group < 100 {
            group *= 100;
            last = Hundred;
        } else if let (Some(v), A | Ones | Teens | Tens | Hundred) = (scale(&w), last) {
            if total != 0 && total <= v * 999 {
                break;
            }
            total += group.max(1) * v;
            group = 0;
            last = Scale;
        } else if w == "and"
            && matches!(last, Hundred | Scale)
            && next.is_some_and(|n| {
                ones(n).is_some()
                    || teens(n).is_some()
                    || tens(n).is_some()
                    || ones_ordinal(n).is_some()
                    || big_ordinal(n).is_some()
            })
        {
            last = And;
        } else if let (Some(v), Tens | Hundred | Scale | And) = (ones_ordinal(&w), last) {
            // "Twenty second" is as often "a twenty second video" as "the twenty second of May".
            let before = (i > 0).then(|| tokens[i - 1].word());
            if w == "second"
                && next != Some("of")
                && !matches!(before.as_deref(), Some("the" | "on"))
            {
                break;
            }
            group += v;
            ordinal = true;
            last = Ordinal;
            j += 1;
            break;
        } else if let (Some(v), Start | Hundred | Scale | And) = (big_ordinal(&w), last) {
            group += v;
            ordinal = true;
            last = Ordinal;
            j += 1;
            break;
        } else {
            break;
        }
        j += 1;
    }
    if matches!(last, Start | A | And) {
        return None;
    }
    let words: Vec<String> = tokens[i..j].iter().map(Token::word).collect();
    let value = total + group;
    let kind = if words.len() == 1 && ones(&words[0]).is_some() {
        Kind::Digit
    } else if !ordinal
        && (10..=99).contains(&value)
        && !words.iter().any(|w| w == "hundred" || scale(w).is_some())
    {
        Kind::Pair
    } else {
        Kind::Number
    };
    Some((
        j - i,
        Number {
            value,
            kind,
            ordinal,
        },
    ))
}

/// A word that's one digit in a string of them, like a phone number: 0 to 9, or "oh".
fn digit_of(token: &Token) -> Option<u64> {
    let w = token.word();
    ones(&w).or((w == "oh").then_some(0))
}

/// Digits said one at a time, like "nine one one" or "double five": at least two of them.
fn digit_string(tokens: &[Token], i: usize) -> Option<(usize, String)> {
    let mut out = String::new();
    let mut j = i;
    while j < tokens.len() && (j == i || joined(tokens, j)) {
        let w = tokens[j].word();
        if w == "double" || w == "triple" {
            if let Some(d) = joined(tokens, j + 1)
                .then(|| ones(&tokens[j + 1].word()))
                .flatten()
            {
                out.push_str(&d.to_string().repeat(if w == "double" { 2 } else { 3 }));
                j += 2;
                continue;
            }
        }
        // "Oh" is only a zero after another digit: on its own it's "oh, that's fine".
        match digit_of(&tokens[j]) {
            Some(_) if w == "oh" && out.is_empty() => break,
            Some(d) => out.push_str(&d.to_string()),
            None => break,
        }
        j += 1;
    }
    (out.len() >= 2).then_some((j - i, out))
}

fn suffix(value: u64) -> &'static str {
    match (value % 100, value % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

/// 10,000 and up get commas. Whole millions and billions keep the word, as in "2 million".
fn written(value: u64) -> String {
    for (name, size) in [("billion", 1_000_000_000), ("million", 1_000_000)] {
        if value >= size && value % size == 0 && value / size < 1000 {
            return format!("{} {name}", value / size);
        }
    }
    let digits = value.to_string();
    if value < 10_000 {
        return digits;
    }
    let mut out = String::new();
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn am_pm(token: &Token) -> bool {
    matches!(token.word().replace('.', "").as_str(), "am" | "pm")
}

/// The text with numbers said in words written as digits.
pub fn to_digits(text: &str) -> String {
    let tokens = split_hyphens(tokens(text));
    if tokens.is_empty() {
        return text.to_string();
    }
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let string = digit_string(&tokens, i);
        let Some((used, number)) = parse(&tokens, i) else {
            match string {
                Some((used, digits)) => {
                    let tail = &tokens[i + used - 1];
                    out.push(Token::new(
                        &tokens[i].lead,
                        &digits,
                        &tail.trail,
                        &tail.space,
                    ));
                    i += used;
                }
                None => {
                    out.push(tokens[i].clone());
                    i += 1;
                }
            }
            continue;
        };
        let (first, end) = (&tokens[i], i + used);
        let last = &tokens[end - 1];
        let value = number.value;
        let follow = |k: usize| joined(&tokens, end + k).then(|| &tokens[end + k]);
        let before = joined(&tokens, i).then(|| tokens[i - 1].word());
        let next = follow(0);
        let next_word = next.map(Token::word);
        let next_word = next_word.as_deref();

        // A second half: "nineteen ninety", "five thirty", "five oh five".
        let mut minutes = None;
        if next.is_some() && !number.ordinal && matches!(number.kind, Kind::Digit | Kind::Pair) {
            match parse(&tokens, end) {
                Some((more, half)) if half.kind == Kind::Pair => minutes = Some((half.value, more)),
                _ => {
                    let digit = follow(1).and_then(|t| ones(&t.word()));
                    if let (Some("oh"), Some(d)) = (next_word, digit) {
                        minutes = Some((d, 2));
                    }
                }
            }
        }
        if let Some((half, more)) = minutes {
            let tail = &tokens[end + more - 1];
            let after = follow(more);
            let timed = after.is_some_and(|t| am_pm(t) || t.word() == "o'clock")
                || before.as_deref().is_some_and(|b| TIME_WORDS.contains(&b));
            if (1..=12).contains(&value) && half <= 59 && (timed || number.kind == Kind::Pair) {
                let time = format!("{value}:{half:02}");
                out.push(Token::new(&first.lead, &time, &tail.trail, &tail.space));
                i = end + more;
                continue;
            }
            // Years: "nineteen ninety nine", "twenty twenty four", "nineteen oh five".
            if number.kind == Kind::Pair && (13..=20).contains(&value) && (half >= 10 || more == 2)
            {
                let year = format!("{value}{half:02}");
                out.push(Token::new(&first.lead, &year, &tail.trail, &tail.space));
                i = end + more;
                continue;
            }
        }
        if let Some((more, digits)) = string.filter(|(more, _)| *more > used) {
            let tail = &tokens[i + more - 1];
            out.push(Token::new(&first.lead, &digits, &tail.trail, &tail.space));
            i += more;
            continue;
        }
        if !number.ordinal {
            // "Two point five": the digits after the point are said one at a time.
            if next_word == Some("point") {
                let mut decimals = String::new();
                let mut k = end + 1;
                while joined(&tokens, k) {
                    let Some(d) = digit_of(&tokens[k]) else { break };
                    decimals.push_str(&d.to_string());
                    k += 1;
                }
                if !decimals.is_empty() {
                    let tail = &tokens[k - 1];
                    let text = format!("{}.{decimals}", written(value));
                    out.push(Token::new(&first.lead, &text, &tail.trail, &tail.space));
                    i = k;
                    continue;
                }
            }
            if let (Some("percent"), Some(unit)) = (next_word, next) {
                let text = format!("{}%", written(value));
                out.push(Token::new(&first.lead, &text, &unit.trail, &unit.space));
                i = end + 1;
                continue;
            }
            if let (Some("dollar" | "dollars"), Some(unit)) = (next_word, next) {
                let mut text = format!("${}", written(value));
                let (mut tail, mut k) = (unit, end + 1);
                let and = joined(&tokens, k) && tokens[k].word() == "and";
                if let Some((used, cents)) = and.then(|| parse(&tokens, k + 1)).flatten() {
                    let unit = k + 1 + used;
                    let said_cents = joined(&tokens, unit)
                        && matches!(tokens[unit].word().as_str(), "cent" | "cents");
                    if cents.value < 100 && !cents.ordinal && said_cents {
                        text.push_str(&format!(".{:02}", cents.value));
                        tail = &tokens[unit];
                        k = unit + 1;
                    }
                }
                out.push(Token::new(&first.lead, &text, &tail.trail, &tail.space));
                i = k;
                continue;
            }
        }
        let unit =
            next.is_some_and(am_pm) || matches!(next_word, Some("o'clock" | "cent" | "cents"));
        if number.kind == Kind::Digit && !unit {
            // One to nine on their own stay words, as in "one of them" or "two kids".
            out.extend_from_slice(&tokens[i..end]);
            i = end;
            continue;
        }
        let text = if number.ordinal {
            format!("{}{}", written(value), suffix(value))
        } else {
            written(value)
        };
        out.push(Token::new(&first.lead, &text, &last.trail, &last.space));
        i = end;
    }
    out.iter()
        .map(|t| format!("{}{}{}{}", t.lead, t.core, t.trail, t.space))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::to_digits;

    /// What dictation should type for each thing heard.
    const HEARD_AND_TYPED: [(&str, &str); 81] = [
        // Phone numbers, codes and other digits said one at a time.
        (
            "My number is nine one two three four five six seven.",
            "My number is 91234567.",
        ),
        (
            "Call nine one two three, four five six seven.",
            "Call 9123, 4567.",
        ),
        ("The code is four four four.", "The code is 444."),
        ("It's double five triple seven.", "It's 55777."),
        ("Dial nine oh one.", "Dial 901."),
        // Ten and up, and anything with hundred or thousand.
        ("I need twenty five copies.", "I need 25 copies."),
        ("About ten minutes.", "About 10 minutes."),
        ("A hundred people came.", "100 people came."),
        (
            "It was a hundred and fifty years ago.",
            "It was 150 years ago.",
        ),
        (
            "There are three thousand five hundred of them.",
            "There are 3500 of them.",
        ),
        ("We raised twenty five thousand.", "We raised 25,000."),
        ("Two million users.", "2 million users."),
        ("One hundred and five.", "105."),
        ("Twenty-five people.", "25 people."),
        ("Fifteen hundred.", "1500."),
        // One to nine on their own stay words.
        (
            "No one came, so one of us left.",
            "No one came, so one of us left.",
        ),
        (
            "I have two kids and three cats.",
            "I have two kids and three cats.",
        ),
        ("One, two, three, go.", "One, two, three, go."),
        ("Oh, that's fine.", "Oh, that's fine."),
        ("Two and a half.", "Two and a half."),
        // Money, percent and decimals.
        ("It costs twenty five dollars.", "It costs $25."),
        ("Five dollars and fifty cents.", "$5.50."),
        ("One dollar.", "$1."),
        ("Fifty percent off.", "50% off."),
        ("Only five percent.", "Only 5%."),
        ("Version two point one.", "Version 2.1."),
        ("Zero point seven five.", "0.75."),
        ("About one point five million.", "About 1.5 million."),
        ("Five cents.", "5 cents."),
        // Times.
        ("Meet at five pm.", "Meet at 5 pm."),
        ("Meet at five thirty pm.", "Meet at 5:30 pm."),
        ("At five thirty.", "At 5:30."),
        ("See you at nine oh five.", "See you at 9:05."),
        ("It starts at eleven thirty.", "It starts at 11:30."),
        ("Twelve fifteen works.", "12:15 works."),
        ("Five o'clock.", "5 o'clock."),
        ("Five p.m. works.", "5 p.m. works."),
        // Years.
        ("Back in nineteen ninety nine.", "Back in 1999."),
        ("In twenty twenty four.", "In 2024."),
        ("Two thousand twenty four.", "2024."),
        ("Nineteen oh five.", "1905."),
        // Ordinals.
        ("On the twenty first.", "On the 21st."),
        ("The twenty second of May.", "The 22nd of May."),
        ("A twenty second video.", "A 20 second video."),
        ("The fifteenth time.", "The 15th time."),
        ("First of all.", "First of all."),
        ("Wait a second.", "Wait a second."),
        ("The one hundred and first.", "The 101st."),
        // Hyphenated.
        ("A forty-two-year-old man.", "A 42-year-old man."),
        ("A five-year-old.", "A five-year-old."),
        ("One-on-one.", "One-on-one."),
        // Already digits, or nothing to do.
        ("I have 25 apples.", "I have 25 apples."),
        ("Hello there.", "Hello there."),
        ("", ""),
        ("Thirty and forty.", "30 and 40."),
        ("Ten four.", "10 four."),
        // Harder ones.
        ("Twenty twenty.", "2020."),
        ("Room two oh four.", "Room 204."),
        (
            "Nine one two three four five six seven, please.",
            "91234567, please.",
        ),
        ("Ninety nine problems.", "99 problems."),
        ("Thirty one days.", "31 days."),
        ("A thousand thanks.", "1000 thanks."),
        ("A million thanks.", "A million thanks."),
        ("Twenty twenty-four.", "2024."),
        ("Twenty-first century.", "21st century."),
        ("The ninety-ninth.", "The 99th."),
        ("Two hundred thousand.", "200,000."),
        ("One billion.", "1 billion."),
        ("Five, six.", "Five, six."),
        ("Twenty, thirty.", "20, 30."),
        ("Eleven.", "11."),
        ("We have twelve, maybe thirteen.", "We have 12, maybe 13."),
        ("Ten dollars and five cents.", "$10.05."),
        ("Three hundred and sixty five days.", "365 days."),
        ("Fifty-fifty.", "50-50."),
        (
            "Twenty \u{201c}five\u{201d} things.",
            "20 \u{201c}five\u{201d} things.",
        ),
        ("Nineteen eighty-four.", "1984."),
        ("Forty two.", "42."),
        ("It's at one.", "It's at one."),
        ("At one thirty.", "At 1:30."),
        ("Twenty first of June.", "21st of June."),
    ];

    #[test]
    fn spoken_numbers_are_typed_as_digits() {
        for (heard, typed) in HEARD_AND_TYPED {
            assert_eq!(to_digits(heard), typed, "heard {heard:?}");
        }
    }

    #[test]
    fn spaces_and_line_breaks_stay_where_they_were() {
        assert_eq!(to_digits("  twenty five\nthirty  six "), "  25\n36 ");
    }
}
