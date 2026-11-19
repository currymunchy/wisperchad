//! Checks on a written reply for the slips a small model makes, each found from the user's note and
//! the message, with what to tell the model so its next try fixes it. Qwen3.5 2B wrote "sorry to hear
//! that your laptop died" for the user's own laptop, turned down the time the user offered and kept
//! theirs, said no when the user said yes, took the other side on a post, left out "maybe next week"
//! and made up times. Asked again, pointedly, it mostly gets them right.

use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// What's wrong with a reply, most serious first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// The user's own news told back to them as theirs: "sorry to hear that your laptop died".
    Flip,
    /// Words from the wording's examples copied in, like "pineapple" on a post about work.
    Example,
    /// Their words that only they can say, like "to help us facilitate the session", as the user's.
    TheirWords,
    /// The time the user offered turned down, and theirs kept.
    Offer,
    /// No, where the note didn't say no.
    SaidNo,
    /// Yes, where the note said no.
    SaidYes,
    /// The other side on a post from the one the note takes.
    Side,
    /// A comment on how the post's author feels, not on what the post says.
    Feelings,
    /// The user's reason left out: "my grandma", "I'm sick".
    Lost,
    /// A time or day the note gave left out.
    Missing,
    /// A time or day nobody gave.
    MadeUp,
    /// A link from their message sent back to them, like the form they asked to be filled in.
    Link,
}

/// One slip, with what goes into the wording that asks for it to be fixed.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub fix: Fix,
    pub values: Vec<(&'static str, String)>,
}

impl Problem {
    fn new(fix: Fix, values: &[(&'static str, String)]) -> Self {
        Self {
            fix,
            values: values.to_vec(),
        }
    }
}

/// Which side of a post the user's note takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    Agree,
    Disagree,
}

fn pattern(cell: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).expect("a valid pattern"))
}

/// Lower case, with the curly apostrophes pages and speech recognition use made straight.
fn plain(text: &str) -> String {
    text.to_lowercase().replace('\u{2019}', "'")
}

/// Whether `word` is in `text` with no letter or digit right before or after it.
fn has_word(text: &str, word: &str) -> bool {
    let alnum = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    text.match_indices(word).any(|(at, _)| {
        !alnum(text[..at].chars().next_back()) && !alnum(text[at + word.len()..].chars().next())
    })
}

const ANCHOR_WORDS: [&str; 22] = [
    "noon",
    "midnight",
    "tonight",
    "tomorrow",
    "today",
    "morning",
    "afternoon",
    "evening",
    "next week",
    "next time",
    "next month",
    "next year",
    "weekend",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "end of day",
    "eod",
];

/// The times, days and numbers some text gives, like "2pm", "tomorrow" or "next week".
pub fn anchors(text: &str) -> Vec<String> {
    static TOMORROW: OnceLock<Regex> = OnceLock::new();
    static NUMBER: OnceLock<Regex> = OnceLock::new();
    let text = plain(text);
    let text = pattern(&TOMORROW, r"\b(?:tmrw|tmr|tomoro|2moro)\b").replace_all(&text, "tomorrow");
    let mut found: Vec<String> = ANCHOR_WORDS
        .iter()
        .filter(|word| has_word(&text, word))
        .map(|word| word.to_string())
        .collect();
    let numbers = pattern(
        &NUMBER,
        r"(?:^|[^\w$])(\$?\d+(?:[:.]\d+)?(?:\s?(?:am|pm|%))?)",
    );
    found.extend(
        numbers
            .captures_iter(&text)
            .map(|caps| caps[1].replace(' ', "")),
    );
    found
}

/// The times and days the note gave that the reply left out.
fn missing_anchors(reply: &str, note: &str) -> Vec<String> {
    let reply = plain(reply);
    let flat = reply.replace(' ', "");
    anchors(note)
        .into_iter()
        .filter(|anchor| {
            let digits = anchor.chars().any(|c| c.is_ascii_digit());
            !(has_word(&reply, anchor) || (digits && flat.contains(anchor.as_str())))
        })
        .collect()
}

/// The times and days the reply gave that neither the note nor their message did.
fn made_up_anchors(reply: &str, note: &str, message: &str) -> Vec<String> {
    let digits = |anchor: &str| {
        let kept: String = anchor
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == ':' || *c == '.')
            .collect();
        if kept.is_empty() {
            anchor.to_string()
        } else {
            kept
        }
    };
    let given: BTreeSet<String> = anchors(note)
        .iter()
        .chain(anchors(message).iter())
        .map(|anchor| digits(anchor))
        .collect();
    let flat = plain(&format!("{note} {message}")).replace(' ', "");
    anchors(reply)
        .into_iter()
        .filter(|anchor| !given.contains(&digits(anchor)) && !flat.contains(anchor.as_str()))
        .collect()
}

/// Words after "my" that aren't the user's things.
const NOT_THINGS: [&str; 7] = ["bad", "god", "goodness", "own", "gosh", "word", "pleasure"];

/// What the note calls the user's own, like "my laptop" or "my grandma", in order.
fn my_things(note: &str) -> BTreeSet<String> {
    static MY: OnceLock<Regex> = OnceLock::new();
    pattern(&MY, r"\bmy ([a-z]+)")
        .captures_iter(&plain(note))
        .map(|caps| caps[1].to_string())
        .filter(|thing| !NOT_THINGS.contains(&thing.as_str()))
        .collect()
}

/// The user's own thing the reply treats as theirs: "your laptop", or sorry to hear about it.
fn flipped(reply: &str, note: &str) -> Option<String> {
    let reply = plain(reply);
    let things = my_things(note);
    for thing in &things {
        if has_word(&reply, &format!("your {thing}")) {
            return Some(thing.clone());
        }
        let sorry = reply
            .split(['.', '!', '?', '\n'])
            .any(|sentence| sentence.contains("sorry to hear") && has_word(sentence, thing));
        if sorry {
            return Some(thing.clone());
        }
    }
    // Sorry to hear anything, when the note's news was the user's own and it said nothing of the sort.
    let note = plain(note);
    let first = things.iter().next()?;
    (reply.contains("sorry to hear") && !note.contains("hear")).then(|| first.clone())
}

/// What the note says of the user's thing, "my laptop died", up to the end of its sentence.
fn clause(note: &str, thing: &str) -> String {
    let note = plain(note);
    let start = format!("my {thing}");
    match note.find(&start) {
        Some(at) => note[at..]
            .split(['.', ',', '!', '?'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_string(),
        None => start,
    }
}

/// Words and phrases that agree with a post, and that don't.
const AGREE: [&str; 15] = [
    "agreed",
    "so true",
    "facts",
    "exactly",
    "well said",
    "spot on",
    "great post",
    "great take",
    "100%",
    "totally",
    "love this",
    "couldn't agree",
    "true that",
    "this is great",
    "nailed it",
];
const DISAGREE: [&str; 19] = [
    "disagree",
    "don't agree",
    "dont agree",
    "not agree",
    "wrong",
    "nonsense",
    "worst take",
    "bad take",
    "not true",
    "rubbish",
    "terrible take",
    "take is bad",
    "is bad",
    "nah",
    "no,",
    "isn't true",
    "doesn't",
    "aren't",
    "isn't",
];

/// The side the note takes on a post, when its words say. A small model wrote what it thought of a
/// post, whatever the user's note said, unless the side was spelled out.
pub fn stance(note: &str) -> Option<Stance> {
    let text = format!(" {} ", plain(note));
    let against =
        DISAGREE.iter().any(|phrase| text.contains(phrase)) || text.trim().starts_with("no ");
    let starts_word = |at: usize| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphabetic)
    };
    let agrees_word = AGREE
        .iter()
        .any(|phrase| text.match_indices(phrase).any(|(at, _)| starts_word(at)));
    // "agree", but not "disagree", "don't agree" or "not agree".
    let agrees = text.match_indices("agree").any(|(at, _)| {
        let before = &text[..at];
        !["dis", "don't ", "dont ", "not "]
            .iter()
            .any(|word| before.ends_with(word))
    });
    match (against, agrees_word || agrees) {
        (true, false) => Some(Stance::Disagree),
        (false, true) => Some(Stance::Agree),
        _ => None,
    }
}

/// Whether the reply takes the other side on the post from the note's.
fn wrong_side(reply: &str, stance: Stance) -> bool {
    let reply = plain(reply);
    match stance {
        Stance::Agree => [
            "disagree",
            "see it differently",
            "however",
            "but i ",
            "though i ",
            "not convinced",
            "i'm not sure",
        ]
        .iter()
        .any(|phrase| reply.contains(phrase)),
        Stance::Disagree => {
            [
                "agree with you",
                "agree with your",
                "agree with the idea",
                "agree with the sentiment",
                "agree with this",
                "you're right",
                "you are right",
            ]
            .iter()
            .any(|phrase| reply.contains(phrase))
                && !reply.contains("disagree")
        }
    }
}

/// A comment that opens on how the post's author feels: "I see you're feeling overwhelmed".
fn about_feelings(reply: &str) -> bool {
    static FEELINGS: OnceLock<Regex> = OnceLock::new();
    pattern(
        &FEELINGS,
        r"^(?:hi[^.!?]*[,!.]\s*)?(?:i see (?:that )?(?:you're|you are|you\b)|i understand (?:that )?you're|you're feeling|it sounds like you)",
    )
    .is_match(&plain(reply))
}

/// Whether the reply says no where the note said nothing like it.
fn says_no_unasked(reply: &str, note: &str) -> bool {
    static NO: OnceLock<Regex> = OnceLock::new();
    static NEGATIVE: OnceLock<Regex> = OnceLock::new();
    static FINE: OnceLock<Regex> = OnceLock::new();
    let says_no = pattern(
        &NO,
        r"\b(?:can't|cannot|won't be able|unable|not able|have something else on)\b",
    );
    let negative = pattern(
        &NEGATIVE,
        r"\b(?:can't|cant|cannot|won't|wont|ain't|aint|nope|no|nah|not|don't|dont|doesn't|doesnt|busy|sick|lazy|broke|unable|too early|too late|but)\b",
    );
    let note = plain(note);
    let note = pattern(&FINE, r"no (?:problem|worries|stress)").replace_all(&note, "");
    says_no.is_match(&plain(reply)) && !negative.is_match(&note)
}

/// Whether the reply goes along with something the note said no to.
/// Ways of saying yes with a "can't" in them.
const KEEN: [&str; 8] = [
    "can't wait",
    "cant wait",
    "cannot wait",
    "can't thank",
    "cant thank",
    "can't believe",
    "cant believe",
    "won't regret",
];

/// Whether the note says no: "can't", "I ain't going", "nope", "No, thanks". Not "I can't wait".
pub fn says_no(note: &str) -> bool {
    static NOTE_NO: OnceLock<Regex> = OnceLock::new();
    let note = KEEN
        .iter()
        .fold(plain(note), |note, keen| note.replace(keen, ""));
    let note = note.trim();
    // "No, ...", "No thanks" and "No way" say no; "no reply yet" and "nothing" don't.
    let opens_no = note.strip_prefix("no").is_some_and(|rest| {
        let then = rest.trim_start();
        rest.is_empty()
            || rest.starts_with([',', '.', '!'])
            || (rest.starts_with(char::is_whitespace)
                && (then.starts_with("thanks") || then.starts_with("way")))
    });
    pattern(
        &NOTE_NO,
        r"\b(?:can't|cant|cannot|won't|wont|ain't|aint|nah|nope|not going|not coming|not gonna|not interested|don't think|dont think|no way|no man|hell no)\b",
    )
    .is_match(note)
        || opens_no
}

fn says_yes_unasked(reply: &str, note: &str) -> bool {
    static DECLINES: OnceLock<Regex> = OnceLock::new();
    let declines = pattern(
        &DECLINES,
        r"\b(?:can't|cannot|won't|unable|not|sorry|no|pass|decline|unfortunately|afraid)\b",
    );
    says_no(note) && !declines.is_match(&plain(reply))
}

/// The time the user offered, as the reply turns it down: "I can't make it by noon" when the
/// note said "I can do noon".
fn turned_down_offer(reply: &str, note: &str) -> Option<String> {
    static OFFER: OnceLock<Regex> = OnceLock::new();
    static CANT: OnceLock<Regex> = OnceLock::new();
    let reply = plain(reply);
    let offers = pattern(
        &OFFER,
        r"(?:i can do|can do|i can make|i'm free|i am free)\s+([^.,!?]+)|([^.,!?]+?)\s+(?:works|is fine|is ok|is okay)\b",
    );
    let cant = pattern(&CANT, r"can't|cannot|unable|not able|won't");
    for caps in offers.captures_iter(&plain(note)) {
        let offer = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map_or("", |m| m.as_str())
            .trim()
            .to_string();
        let mut wanted = anchors(&offer);
        if wanted.is_empty() {
            wanted.extend(
                offer
                    .split_whitespace()
                    .filter(|word| word.chars().count() > 2)
                    .next_back()
                    .map(String::from),
            );
        }
        for anchor in wanted {
            // What follows each "can't", to the end of its clause or a "but".
            let turned_down = cant.find_iter(&reply).any(|m| {
                let rest = &reply[m.end()..];
                let clause = rest
                    .split(['.', '!', '?', ',', ';'])
                    .next()
                    .unwrap_or_default();
                let clause = clause.split(" but ").next().unwrap_or_default();
                let clause: String = clause.chars().take(31).collect();
                has_word_start(&clause, &anchor)
            });
            if turned_down {
                return Some(anchor);
            }
        }
    }
    None
}

/// Whether `word` starts a word somewhere in `text`.
fn has_word_start(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
    })
}

/// Reasons the note gives, and the words that keep them in a reply.
const REASONS: [(&str, &[&str]); 3] = [
    (
        "sick",
        &[
            "sick",
            "unwell",
            "not feeling well",
            "under the weather",
            "ill",
            "not well",
        ],
    ),
    ("hospital", &["hospital"]),
    (
        "broke",
        &["broke", "money", "cash", "funds", "afford", "lend"],
    ),
];

/// The user's own things and reasons the reply left out, like "grandma" or "sick".
fn missing_reasons(reply: &str, note: &str) -> Vec<String> {
    let reply = plain(reply);
    let note_text = plain(note);
    let mut lost: Vec<String> = my_things(note)
        .into_iter()
        .filter(|thing| !has_word_start(&reply, thing))
        .collect();
    for (reason, kept) in REASONS {
        if has_word(&note_text, reason) && !kept.iter().any(|word| reply.contains(word)) {
            lost.push(reason.to_string());
        }
    }
    lost
}

/// "\"8am\", \"noon\"", for a hint.
fn quoted(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A run of their words with "us", "we" or "our" in it, which only they can say: a small model wrote
/// "I will submit my idea to help us facilitate this event", from "To help us facilitate the session".
fn their_words(reply: &str, message: &str) -> Option<String> {
    let split = |text: &str| -> Vec<String> {
        plain(text)
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|word| !word.is_empty())
            .map(String::from)
            .collect()
    };
    let (reply, message) = (split(reply), split(message));
    for size in [5, 4, 3] {
        for run in message.windows(size) {
            let theirs = run
                .iter()
                .any(|word| ["us", "we", "our"].contains(&word.as_str()));
            if theirs && reply.windows(size).any(|seen| seen == run) {
                return Some(run.join(" "));
            }
        }
    }
    None
}

/// What only the sender says, speaking for their company. A small model answered a bank's
/// newsletter, whose footer said "Suspect fraud? We're here for you", with "We are here to help.
/// Please contact us directly", in the user's name.
const SENDERS_LINES: [&str; 28] = [
    "contact us",
    "reach out to us",
    "reach us",
    "call us",
    "chat with us",
    "email us",
    "write to us",
    "get in touch with us",
    "visit us",
    "we are here",
    "we're here",
    "we are happy to",
    "we're happy to",
    "we'd be happy to",
    "we would be happy to",
    "we are glad to",
    "we're glad to",
    "we are pleased to",
    "we're pleased to",
    "our customers",
    "our services",
    "our products",
    "our app",
    "our support",
    "our website",
    "let us know",
    "thank you for choosing",
    "valued customer",
];

/// Words in a note that make the user one of a group, who can say "contact us" for it.
const OWN_GROUP: [&str; 8] = ["we", "us", "our", "ours", "we're", "we'll", "we've", "let's"];

/// A line from `SENDERS_LINES` in the reply, which the user, one person replying, wouldn't write.
/// Never when the note says "we", "us" or "our" itself, as someone writing for their team does.
pub fn speaks_for_them(reply: &str, note: &str) -> Option<&'static str> {
    let note = plain(note);
    if OWN_GROUP.iter().any(|word| has_word(&note, word)) {
        return None;
    }
    let reply = plain(reply);
    SENDERS_LINES
        .iter()
        .copied()
        .find(|line| has_word(&reply, line))
}

/// Words a note can hold and still only say thanks, like "okay, thank you so much".
const THANKS_NOTE: [&str; 24] = [
    "thanks", "thank", "thx", "ty", "cheers", "appreciate", "appreciated", "you", "so", "very",
    "much", "a", "lot", "many", "it", "ok", "okay", "alright", "noted", "great", "got", "bro",
    "man", "mate",
];
/// The words in `THANKS_NOTE` that do the thanking.
const THANKING: [&str; 6] = ["thanks", "thank", "thx", "ty", "cheers", "appreciate"];

/// Whether the note does nothing but thank them: "thank you", "okay thanks", "cheers mate".
pub fn only_thanks(note: &str) -> bool {
    let note = plain(note);
    let words: Vec<&str> = note
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .collect();
    words.iter().any(|word| THANKING.contains(word))
        && words.iter().all(|word| THANKS_NOTE.contains(word))
}

/// Words that make a sender a team or a company, not a person to greet by name.
const NOT_A_PERSON: [&str; 30] = [
    "team",
    "careers",
    "career",
    "support",
    "service",
    "services",
    "noreply",
    "no-reply",
    "info",
    "hr",
    "admin",
    "sales",
    "office",
    "finance",
    "department",
    "desk",
    "help",
    "notifications",
    "bank",
    "ltd",
    "inc",
    "pte",
    "llc",
    "group",
    "account",
    "accounts",
    "billing",
    "recruitment",
    "talent",
    "ap",
];
/// Small words around a sender's name, in lower case, which are never the name itself.
const LINKING_WORDS: [&str; 9] = ["by", "from", "via", "of", "on", "at", "the", "and", "to"];
/// Titles kept with the name after them: "Dr Tan".
const TITLES: [&str; 7] = ["mr", "ms", "mrs", "dr", "prof", "madam", "mdm"];
/// Parts of an address that aren't a person's.
const NOT_A_PERSON_ADDRESS: [&str; 12] = [
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "info@",
    "support",
    "team",
    "careers",
    "hello@",
    "contact",
    "notifications",
    "news",
];

/// What an email to `sender` greets them by, when they're a person: "Farahnadhia" for
/// "Farahnadhia RAHMAN from.TP (DTSA) <...>", "Dr Tan" for "Dr Tan Wei Ming". Nothing for a team or
/// a company, like "Early Careers AP" or "Finance Team".
pub fn first_name(sender: &str) -> Option<String> {
    let (name, address) = sender.split_once('<').unwrap_or((sender, ""));
    let address = address.to_lowercase();
    if NOT_A_PERSON_ADDRESS
        .iter()
        .any(|part| address.contains(part))
    {
        return None;
    }
    let mut kept = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = kept
        .split_whitespace()
        .map(|word| word.trim_end_matches('.'))
        .filter(|word| {
            // "teohkayi@example.com, by teohkayi", as a small model wrote a sender, greeted "Hi by,".
            !LINKING_WORDS.contains(word)
                && word.chars().count() >= 2
                && word.starts_with(char::is_alphabetic)
                && word
                    .chars()
                    .all(|c| c.is_alphabetic() || c == '\'' || c == '-')
        })
        .collect();
    if words.is_empty()
        || words
            .iter()
            .any(|word| NOT_A_PERSON.contains(&word.to_lowercase().as_str()))
    {
        return None;
    }
    match words.as_slice() {
        [title, name, ..] if TITLES.contains(&title.to_lowercase().as_str()) => {
            Some(format!("{title} {name}"))
        }
        [name, ..] => Some(name.to_string()),
        [] => None,
    }
}

/// Words only the examples in reply-prompts.json use. In a reply, they're an example copied in, as
/// when a small model commented "pineapple just doesn't belong on pizza" on a post about work.
const EXAMPLE_WORDS: [&str; 5] = [
    "pineapple",
    "pizza",
    "rate our service",
    "role has been filled",
    "join the call",
];

/// Whether the reply has words from the examples that neither the note nor their message has.
fn copied_example(reply: &str, note: &str, message: &str) -> bool {
    let reply = plain(reply);
    let given = plain(&format!("{note} {message}"));
    EXAMPLE_WORDS
        .iter()
        .any(|words| reply.contains(words) && !given.contains(words))
}

/// What's wrong with a reply, most serious first. `message` is what they wrote; `comment` is
/// whether the reply is a comment on a post.
pub fn problems(reply: &str, note: &str, message: &str, comment: bool) -> Vec<Problem> {
    let mut found = Vec::new();
    if let Some(thing) = flipped(reply, note) {
        found.push(Problem::new(Fix::Flip, &[("clause", clause(note, &thing))]));
    }
    if copied_example(reply, note, message) {
        found.push(Problem::new(Fix::Example, &[]));
    }
    let theirs = their_words(reply, message)
        .or_else(|| speaks_for_them(reply, note).map(String::from));
    if let Some(words) = theirs {
        found.push(Problem::new(Fix::TheirWords, &[("words", words)]));
    }
    if let Some(offer) = turned_down_offer(reply, note) {
        let message_flat = plain(message).replace(' ', "");
        let theirs = anchors(note).into_iter().find(|anchor| {
            *anchor != offer
                && !offer.contains(anchor.as_str())
                && message_flat.contains(anchor.as_str())
        });
        found.push(Problem::new(
            Fix::Offer,
            &[("offer", offer), ("theirs", theirs.unwrap_or_default())],
        ));
    }
    if !comment && says_no_unasked(reply, note) {
        found.push(Problem::new(Fix::SaidNo, &[]));
    }
    if !comment && says_yes_unasked(reply, note) {
        found.push(Problem::new(Fix::SaidYes, &[]));
    }
    if comment {
        if let Some(side) = stance(note).filter(|side| wrong_side(reply, *side)) {
            let side = match side {
                Stance::Agree => "agree",
                Stance::Disagree => "disagree",
            };
            found.push(Problem::new(Fix::Side, &[("side", side.to_string())]));
        }
        if about_feelings(reply) {
            found.push(Problem::new(Fix::Feelings, &[]));
        }
    }
    let lost = missing_reasons(reply, note);
    if !lost.is_empty() {
        found.push(Problem::new(Fix::Lost, &[("what", quoted(&lost))]));
    }
    let missing = missing_anchors(reply, note);
    if !missing.is_empty() {
        found.push(Problem::new(Fix::Missing, &[("what", quoted(&missing))]));
    }
    let made_up = made_up_anchors(reply, note, message);
    if !made_up.is_empty() {
        found.push(Problem::new(Fix::MadeUp, &[("what", quoted(&made_up))]));
    }
    if let Some(link) = their_link(reply, message, note) {
        found.push(Problem::new(Fix::Link, &[("link", link)]));
    }
    found
}

/// A link from their message in the reply, which the note never asked for: a small model answered
/// an angry "I ain't going" by sending DTSA its own form, "https://go.gov.sg/demoday101prepform".
pub fn their_link(reply: &str, message: &str, note: &str) -> Option<String> {
    let note = plain(note);
    let asked = ["link", "links", "website", "site", "url", "form"];
    if asked.iter().any(|word| has_word(&note, word)) || note.contains("http") || note.contains("www") {
        return None;
    }
    let message = plain(message);
    reply
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/'))
        .find(|word| is_link(word) && message.contains(&plain(word)))
        .map(String::from)
}

/// "https://go.gov.sg/form", "www.dtsa.gov.sg" or "go.gov.sg/form", but not "3.45pm/4pm".
fn is_link(word: &str) -> bool {
    let word = plain(word);
    let address = word
        .strip_prefix("https://")
        .or_else(|| word.strip_prefix("http://"))
        .unwrap_or(&word);
    let host = address.split('/').next().unwrap_or_default();
    let named = host.rsplit_once('.').is_some_and(|(name, end)| {
        !name.is_empty() && end.len() >= 2 && end.chars().all(|c| c.is_ascii_alphabetic())
    });
    named && (address.len() > host.len() || word.starts_with("http") || host.starts_with("www."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixes(reply: &str, note: &str, message: &str, comment: bool) -> Vec<Fix> {
        problems(reply, note, message, comment)
            .into_iter()
            .map(|problem| problem.fix)
            .collect()
    }

    #[test]
    fn times_days_and_numbers_are_read_from_the_note() {
        assert_eq!(
            anchors("9 is too early, tell her I can do 2pm."),
            ["9", "2pm"]
        );
        assert_eq!(
            anchors("Can't today. Tomorrow morning okay?"),
            ["tomorrow", "today", "morning"]
        );
        assert_eq!(anchors("wanna grab lunch tmr?"), ["tomorrow"]);
        assert_eq!(anchors("he wants it by 8 am"), ["8am"]);
        assert!(anchors("Sure, no problem.").is_empty());
    }

    #[test]
    fn the_users_own_news_told_back_as_theirs_is_caught() {
        let note = "Sorry, my laptop died. Can I still get full marks?";
        let message = "Your assignment was late. Please explain.";
        let flip = problems(
            "Hi Dr Tan,\n\nI am very sorry to hear that your laptop died.",
            note,
            message,
            false,
        );
        assert_eq!(flip[0].fix, Fix::Flip);
        assert_eq!(flip[0].values, [("clause", "my laptop died".to_string())]);
        assert_eq!(
            fixes(
                "Sorry to hear about your grandma. Hope she gets better.",
                "Sorry bro, my grandma is in the hospital. Can't make it.",
                "you coming to my bday tonight?",
                false
            )[0],
            Fix::Flip
        );
        assert!(!fixes(
            "Hi Dr Tan,\n\nUnfortunately, my laptop died, so I submitted late. Could I still get full marks?",
            note,
            message,
            false
        )
        .contains(&Fix::Flip));
        // Their bad news, with a kind note, may be sorry to hear.
        assert!(fixes(
            "I'm so sorry to hear about your dog.",
            "aw that's sad, sending love",
            "My dog died yesterday.",
            false
        )
        .is_empty());
    }

    #[test]
    fn the_time_the_user_offered_is_never_turned_down() {
        let note = "this person wants it by 8am. no, I can do noon.";
        let message = "Ka Yi, the report is not good enough. Redo it by 8am tomorrow.";
        let wrong = problems(
            "I can't make it by noon, but I'll get it done by 8am tomorrow.",
            note,
            message,
            false,
        );
        assert_eq!(wrong[0].fix, Fix::Offer);
        assert_eq!(
            wrong[0].values,
            [("offer", "noon".to_string()), ("theirs", "8am".to_string())]
        );
        assert!(problems(
            "I can't do 8am, but I can have it to you by noon.",
            note,
            message,
            false
        )
        .is_empty());
        assert!(!fixes(
            "8 works for me.",
            "7 doesn't work, 8 is fine.",
            "can we meet at 7?",
            false
        )
        .contains(&Fix::Offer));
    }

    #[test]
    fn yes_and_no_follow_the_note() {
        let rent = "Hi, rent is due tomorrow. Please transfer by 6pm.";
        assert!(fixes(
            "I'm sorry, I can't make it tomorrow.",
            "Shit, I forgot. I'll send it tonight.",
            rent,
            false
        )
        .contains(&Fix::SaidNo));
        assert!(problems(
            "I'll send it over tonight.",
            "I forgot. I'll send it tonight.",
            rent,
            false
        )
        .is_empty());
        assert!(fixes(
            "Sounds good, I'll see you next week.",
            "Can't make it bro, maybe next week.",
            "yo you coming for dinner tonight?",
            false
        )
        .contains(&Fix::SaidYes));
        // "No problem" is a yes, and "no reply yet" isn't a no.
        assert!(problems(
            "Sure, no problem.",
            "Okay sure, no problem.",
            "cover my shift?",
            false
        )
        .is_empty());
        assert!(!says_yes_unasked(
            "I'm following up on my email.",
            "no reply yet, follow up politely"
        ));
        assert!(says_yes_unasked("Sure, sounds good.", "No thanks."));
    }

    #[test]
    fn a_comment_keeps_the_notes_side() {
        assert_eq!(stance("Great post man, I agree."), Some(Stance::Agree));
        assert_eq!(stance("So true."), Some(Stance::Agree));
        assert_eq!(
            stance("Nah, remote works fine for me."),
            Some(Stance::Disagree)
        );
        assert_eq!(stance("I don't agree at all"), Some(Stance::Disagree));
        assert_eq!(
            stance("This is nonsense, juniors aren't going anywhere."),
            Some(Stance::Disagree)
        );
        assert_eq!(stance("congrats this looks amazing"), None);
        let post = "Remote work kills productivity.";
        let side = problems(
            "I see it differently: remote work suits many people.",
            "Totally agree, the office is way better.",
            post,
            true,
        );
        assert_eq!(side[0].fix, Fix::Side);
        assert_eq!(side[0].values, [("side", "agree".to_string())]);
        assert!(fixes(
            "I see you're feeling overwhelmed by the pressure.",
            "This is the worst take I've ever seen. Burnout is real.",
            post,
            true
        )
        .contains(&Fix::Feelings));
        assert!(!about_feelings("I see your point, but burnout is real."));
    }

    #[test]
    fn their_own_words_are_never_the_users() {
        let message = "To help us facilitate the session, we would love to hear about your idea!";
        let wrong = problems(
            "Hi,\n\nI will submit my idea to help us facilitate this event better.",
            "Thanks, I'll submit my idea before the workshop.",
            message,
            false,
        );
        assert_eq!(wrong[0].fix, Fix::TheirWords);
        assert_eq!(
            wrong[0].values,
            [("words", "to help us facilitate".to_string())]
        );
        assert_eq!(
            their_words("I will submit my idea before the workshop.", message),
            None
        );
    }

    #[test]
    fn people_are_greeted_by_name_and_teams_are_not() {
        let name = |sender| first_name(sender);
        assert_eq!(
            name("Farahnadhia RAHMAN from.TP (DTSA) <Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg>")
                .as_deref(),
            Some("Farahnadhia")
        );
        assert_eq!(
            name("Dr Tan Wei Ming <weiming.tan@uni.edu.sg>").as_deref(),
            Some("Dr Tan")
        );
        assert_eq!(
            name("Rachel Lim <rachel@partnerco.com>").as_deref(),
            Some("Rachel")
        );
        assert_eq!(name("Early Careers AP <EarlyCareersAP@acme.com>"), None);
        assert_eq!(name("Finance Team <finance@acme.example>"), None);
        assert_eq!(name("Shopee <no-reply@shopee.sg>"), None);
        assert_eq!(name("rachel@partnerco.com"), None);
        assert_eq!(name(""), None);
        assert_eq!(name("teohkayi@example.com, by teohkayi").as_deref(), Some("teohkayi"));
        assert_eq!(name("GXS Bank (reply-NNU5T55TI3QE7AA2...)"), None);
    }

    #[test]
    fn a_thanks_never_speaks_for_the_bank() {
        // The reply a small model wrote to a bank's newsletter for "thank you".
        let reply = "Hi,\n\nThank you for reaching out. We are here to help. Please contact us directly if you have any questions or concerns.";
        let message = "Suspect fraud? We're here for you. Chat with us, call us, or reach out to us.";
        let found = problems(reply, "thank you", message, false);
        assert_eq!(found[0].fix, Fix::TheirWords);
        assert_eq!(found[0].values, [("words", "contact us".to_string())]);
        assert_eq!(speaks_for_them("We’re here for you.", "thanks"), Some("we're here"));
        // Writing for their own team, the user says it themselves.
        assert_eq!(speaks_for_them(reply, "tell them we can help, contact us anytime"), None);
        assert_eq!(speaks_for_them("Thanks, I'll keep that in mind.", "thanks"), None);
        assert_eq!(speaks_for_them("We're here at the venue.", "we're here already"), None);
    }

    #[test]
    fn a_note_of_thanks_is_told_apart() {
        for note in ["thank you", "Okay, thanks!", "thanks a lot bro", "cheers mate", "Thank you so much."] {
            assert!(only_thanks(note), "{note}");
        }
        for note in ["thanks, see you Monday", "no thanks", "ok", "thanks but I can't make it", ""] {
            assert!(!only_thanks(note), "{note}");
        }
    }

    #[test]
    fn their_own_link_isnt_sent_back_unasked() {
        let message = "Submit your idea here: https://go.gov.sg/demoday101prepform\nwww.dtsa.gov.sg";
        let reply = "Hi,\n\nPlease find the link below:\nhttps://go.gov.sg/demoday101prepform\n\nBest regards,";
        let found = problems(reply, "Okay thanks.", message, false);
        assert_eq!(found[0].fix, Fix::Link);
        assert_eq!(
            found[0].values,
            [("link", "https://go.gov.sg/demoday101prepform".to_string())]
        );
        // Asked for, or not theirs, it stays; a time with a slash isn't a link.
        assert_eq!(their_link(reply, message, "send them the form link"), None);
        assert_eq!(their_link("See https://example.com/x", message, "ok"), None);
        assert_eq!(their_link("From 3.45pm/4pm, ok", "From 3.45pm/4pm", "ok"), None);
        assert!(is_link("www.dtsa.gov.sg") && is_link("go.gov.sg/demoday101prepform"));
        assert!(!is_link("3.45pm/4pm") && !is_link("e.g.") && !is_link("and/or"));
    }

    #[test]
    fn a_keen_cant_isnt_a_no() {
        assert!(says_no("I can't make it, sorry") && says_no("Boy, I ain't going boy."));
        assert!(says_no("Boy what boy I ain't good boy no man") && says_no("Nope"));
        assert!(!says_no("I can't wait to see you!") && !says_no("can't thank you enough"));
        assert!(!says_no("Okay thanks, see you there"));
    }

    #[test]
    fn an_angry_no_isnt_answered_with_a_yes() {
        let reply = "Hi,\n\nThank you for confirming my registration. I look forward to the session.";
        for note in ["Boy what boy I ain't good boy no man", "Boy, I ain't going boy.", "nope"] {
            assert!(says_yes_unasked(reply, note), "{note}");
        }
        let declined = "Hi,\n\nThank you for the details. Unfortunately, I won't be able to attend.";
        assert!(!says_yes_unasked(declined, "Boy, I ain't going boy."));
        // Nor is that no taken for one the note didn't say, and asked for again.
        let cannot = "Hi,\n\nUnfortunately, I cannot attend the event on Wednesday.";
        assert!(!says_no_unasked(cannot, "Boy, I ain't going boy."));
        assert!(!says_no_unasked(cannot, "Nope"));
    }

    #[test]
    fn an_example_copied_in_is_caught() {
        let post = "Hot take: if you're not working 80 hours a week, you're wasting your life.";
        assert_eq!(
            fixes(
                "I see it differently: pineapple just doesn't belong on pizza for me.",
                "this take is bad",
                post,
                true
            ),
            [Fix::Example]
        );
        // Unless it's what they're talking about.
        assert!(!copied_example(
            "Pineapple on pizza is great!",
            "facts",
            "Pineapple belongs on pizza."
        ));
    }

    #[test]
    fn what_the_note_gives_is_kept_and_nothing_is_made_up() {
        let lost = problems(
            "I can't make it tonight, but let's catch up soon.",
            "Sorry bro, my grandma is in the hospital. Can't make it.",
            "you coming to my bday tonight?",
            false,
        );
        assert_eq!(lost[0].fix, Fix::Lost);
        assert_eq!(
            lost[0].values,
            [("what", "\"grandma\", \"hospital\"".to_string())]
        );
        let missing = problems(
            "Sorry, I can't make it then, I have something else on.",
            "Nah, too lazy. Maybe next time.",
            "can you help me move house this sunday?",
            false,
        );
        assert_eq!(missing[0].fix, Fix::Missing);
        assert_eq!(missing[0].values, [("what", "\"next time\"".to_string())]);
        let made_up = problems(
            "Sorry, I'm not feeling well. Maybe next week?",
            "Can't, I'm sick.",
            "wanna grab lunch tmr?",
            false,
        );
        assert_eq!(made_up[0].fix, Fix::MadeUp);
        assert_eq!(made_up[0].values, [("what", "\"next week\"".to_string())]);
        // Their own days and "10 percent" as "10%" aren't made up.
        assert!(made_up_anchors(
            "Sorry, I can't make it tomorrow.",
            "Can't, I'm sick.",
            "wanna grab lunch tmr?"
        )
        .is_empty());
        assert!(made_up_anchors(
            "I can offer you 10% off.",
            "offer her 10 percent off",
            "It was 3 days late."
        )
        .is_empty());
    }
}
