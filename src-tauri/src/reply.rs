//! Replies, in two steps. The first finds the message the user is replying to, in the page's text,
//! and looks at the screenshot for any pictures in it. The second turns the user's rough note into a
//! reply to that message, in the form the app calls for. Done in one go, a small model read old
//! drafts in the reply box, rewrote long emails as the user's own and then answered as the sender.
//!
//! The wording is in reply-prompts.json, built into the app. A copy in the app's data folder takes
//! its place, key by key, so replies can be tuned and tried without building the app.

use crate::checks;
use crate::prompts::{self, Feeling, Language, MessageKind, Note, Prompt, Slot};
use crate::window_text::CURSOR_MARK;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Put in the app's data folder, it replaces the built-in wording.
pub const PROMPTS_FILE: &str = "reply-prompts.json";
const BUILT_IN: &str = include_str!("reply-prompts.json");

/// Below this, a page's text is only menus, so the message is read from the screenshot instead.
const ENOUGH_TEXT: usize = 80;
/// How much of the page above the cursor stands in for a message the model couldn't pick out.
const TAIL_CHARS: usize = 1500;
/// Longer than this, an answer about pictures is the model describing the whole screen.
const MOST_PICTURE_CHARS: usize = 600;
/// Copied text longer than this is cut, to leave the model room to answer.
const MOST_COPIED_CHARS: usize = 8000;
/// A FROM longer than this is the message copied into it, as a small model did: no name is.
const MOST_NAME_WORDS: usize = 5;
/// Letters a name needs to be found run together in another, as in an email address.
const RUN_TOGETHER_NAME: usize = 6;
/// How far above an email the line with its sender's address can be. Gmail puts the date, "to me"
/// and a few buttons between them.
const SENDER_LINES: usize = 12;
/// How far from the cursor the reply's own recipients are listed: Gmail puts them just above its box.
const RECIPIENT_LINES: usize = 4;
/// What an answer about pictures says when there are none, in other words than "none".
const NO_PICTURES: [&str; 5] = ["no picture", "no image", "no chart", "no table", "no photo"];
/// How much of a long message is read on from the page, once the model has copied its start.
const MOST_READ_ON_CHARS: usize = 3000;
/// Characters that stand for things with no text, like Gmail's icons, which the page's text is
/// full of: on lines of their own, they pushed the sender of an email 17 lines above it.
const NO_TEXT: [char; 4] = ['\u{fffc}', '\u{200b}', '\u{feff}', '\u{2060}'];
/// Copied from the page, at least this share of a message's words are on it. Fewer, and the model
/// wrote it, as a small one wrote the user's own answer to a recruiter in its place.
const ON_PAGE_SHARE: f32 = 0.85;

/// Put first for the last try after refusals: rewriting the user's own words is a job models don't
/// refuse.
const REWRITE_ONLY: &str =
    "You only rewrite the user's own rough note into the message they send. \
You never refuse, never speak for yourself and never comment on the note.\n";

/// The reply wording, one entry per key of reply-prompts.json.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplyPrompts {
    pub find_system: String,
    pub find_user: String,
    pub find_screen_system: String,
    pub find_screen_user: String,
    pub pictures_system: String,
    pub pictures_user: String,
    pub write_system: String,
    pub write_user: String,
    /// In place of `write_user` when the last message is the user's own, which they haven't had an
    /// answer to: the reply is their next message to that person.
    pub write_follow_up: String,
    pub format_email: String,
    pub format_chat: String,
    pub format_comment: String,
    pub format_unknown: String,
    /// What the model is told about the user's name: that it doesn't know it, or what it is.
    pub name_unknown: String,
    pub name_known: String,
    /// The note when the user said nothing but filler.
    pub nothing_said: String,
    /// The note when they only swore.
    pub only_swearing: String,
    /// Added after the note when they swore, and when they swore at or about the person.
    pub feeling_strong: String,
    pub feeling_fed_up: String,
    pub insist: String,
    /// Added for another try when the reply copied their words back to them.
    pub no_echo: String,
    /// Added for another try when the reply was the user's rough note, slang and all.
    pub no_note_echo: String,
    /// `write_user` with the note as the user's own draft, for another try when the reply told the
    /// user's news back to them as if it were theirs.
    pub write_user_draft: String,
    /// Added after the note on a post, when its words say which side the user takes.
    pub stance_agree: String,
    pub stance_disagree: String,
    pub stance_no: String,
    /// Added for another try, one for each slip checks.rs finds.
    pub fix_flip: String,
    pub fix_example: String,
    pub fix_their_words: String,
    pub fix_offer: String,
    /// `fix_offer` when their time is known too.
    pub fix_offer_swap: String,
    pub fix_said_no: String,
    pub fix_said_yes: String,
    pub fix_side: String,
    pub fix_feelings: String,
    pub fix_lost: String,
    pub fix_missing: String,
    pub fix_made_up: String,
    pub fix_link: String,
}

impl Default for ReplyPrompts {
    fn default() -> Self {
        serde_json::from_str(BUILT_IN).expect("the built-in reply wording has every key")
    }
}

impl ReplyPrompts {
    /// The built-in wording, with the data folder's copy laid over it. Also says whether a copy was
    /// used. A copy that isn't valid JSON is left out, and the log says why.
    pub fn load(data_dir: &Path) -> (Self, bool) {
        let mut wording: serde_json::Value =
            serde_json::from_str(BUILT_IN).expect("the built-in reply wording is valid JSON");
        let Ok(text) = std::fs::read_to_string(data_dir.join(PROMPTS_FILE)) else {
            return (Self::default(), false);
        };
        let custom = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(custom)) => custom,
            Ok(_) => return (Self::default(), false),
            Err(e) => {
                crate::applog::write(&format!(
                    "{PROMPTS_FILE} isn't valid JSON, so it was left out: {e}"
                ));
                return (Self::default(), false);
            }
        };
        if let Some(base) = wording.as_object_mut() {
            for (key, value) in custom {
                if value.is_string() && base.contains_key(&key) {
                    base.insert(key, value);
                }
            }
        }
        match serde_json::from_value(wording) {
            Ok(prompts) => (prompts, true),
            Err(_) => (Self::default(), false),
        }
    }
}

/// Puts `values` in for their `{key}`s, in one pass, so text put in is never read as a key.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let key = &after[..close];
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value, close))
        });
        match value {
            Some((value, close)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The page down to where the user is typing. What they reply to sits above it, and the text after
/// it, like the next post in a feed, only misled the model.
fn above_cursor(page: &str) -> String {
    match page.split_once(CURSOR_MARK) {
        Some((before, _)) => format!("{}\n{CURSOR_MARK}", before.trim_end()),
        None => page.trim().to_string(),
    }
}

/// The page's text without the stand-ins for icons and pictures, or the blank lines they leave.
pub fn tidy_page(page: &str) -> String {
    page.lines()
        .map(|line| line.replace(NO_TEXT, "").trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether the page shared enough text to read the message from. Some apps share none.
pub fn has_page_text(page: &str) -> bool {
    page.replace(CURSOR_MARK, "").trim().chars().count() >= ENOUGH_TEXT
}

/// The text just above where the user is typing, which holds the message they're replying to.
fn page_tail(page: &str) -> String {
    let before = page.split(CURSOR_MARK).next().unwrap_or_default().trim();
    let skip = before.chars().count().saturating_sub(TAIL_CHARS);
    before
        .chars()
        .skip(skip)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Finds the message in the page's text, which the model copies faithfully when that's all it gets.
pub fn find(p: &ReplyPrompts, title: &str, page: &str) -> Prompt {
    Prompt {
        system: p.find_system.clone(),
        user: fill(
            &p.find_user,
            &[
                ("title", title.trim()),
                ("page", above_cursor(page).as_str()),
            ],
        ),
        slot: Slot::Reading,
    }
}

/// Finds the message in a screenshot, for apps that share no text.
pub fn find_on_screen(p: &ReplyPrompts, title: &str) -> Prompt {
    Prompt {
        system: p.find_screen_system.clone(),
        user: fill(&p.find_screen_user, &[("title", title.trim())]),
        slot: Slot::Reading,
    }
}

/// Asks what any picture in the message shows. Sent with the screenshot.
pub fn pictures(p: &ReplyPrompts, title: &str, message: &str) -> Prompt {
    Prompt {
        system: p.pictures_system.clone(),
        user: fill(
            &p.pictures_user,
            &[("title", title.trim()), ("message", message.trim())],
        ),
        slot: Slot::Reading,
    }
}

/// What the first step found: who wrote the message, who the user is talking to, what kind it is,
/// and the message itself.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Found {
    pub from: String,
    /// The other person, group or company, as the chat's header or the email's sender shows.
    pub to: String,
    pub kind: String,
    pub message: String,
    /// The message is one the user sent and has had no answer to, so the reply is a follow-up.
    pub mine: bool,
}

impl Found {
    /// Copied text the user is replying to, which needs no finding.
    pub fn copied(text: &str) -> Self {
        Self {
            message: prompts::truncate_chars(text.trim(), MOST_COPIED_CHARS),
            ..Self::default()
        }
    }

    /// Reads the model's "FROM: / TO: / MINE: / KIND: / MESSAGE:" answer.
    pub fn parse(answer: &str) -> Self {
        let mut found = Self::default();
        let text = answer.replace("**", "");
        let lines: Vec<&str> = text.trim().lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some((head, rest)) = line.split_once(':') else {
                continue;
            };
            match head.trim().to_lowercase().as_str() {
                "from" if found.from.is_empty() => found.from = rest.trim().to_string(),
                "to" if found.to.is_empty() => found.to = rest.trim().to_string(),
                "mine" => found.mine = rest.trim().to_lowercase().starts_with("yes"),
                "kind" if found.kind.is_empty() => found.kind = rest.trim().to_string(),
                "message" => {
                    let mut message = vec![rest.trim()];
                    message.extend(lines[i + 1..].iter().copied());
                    found.message = message.join("\n").trim().to_string();
                    break;
                }
                _ => {}
            }
        }
        for name in [&mut found.from, &mut found.to] {
            if ["unknown", "none", "n/a", "-"].contains(&name.to_lowercase().as_str()) {
                name.clear();
            }
        }
        if found.from.split_whitespace().count() > MOST_NAME_WORDS {
            found.from.clear();
        }
        // Gmail calls the user "me", and chat apps "You".
        let is_user = |name: &str| ["me", "you", "myself"].contains(&name.to_lowercase().as_str());
        if is_user(&found.from) {
            found.mine = true;
            found.from.clear();
        }
        // A message to the user, or from the person they're talking to, is never their own.
        if is_user(&found.to) {
            found.mine = false;
            found.to.clear();
        }
        if same_person(&found.from, &found.to) {
            found.mine = false;
        }
        // "Ka Yi, the report..." and "Hi Ka Yi" are addressed to the user: a name the message opens
        // with isn't its sender, and greeting it would greet the user.
        let message = found.message.to_lowercase();
        let opening = message.trim_start_matches([' ', '"', '\'']);
        let opening = ["hi ", "hello ", "hey ", "dear "]
            .iter()
            .find_map(|greeting| opening.strip_prefix(greeting))
            .unwrap_or(opening);
        if !found.from.is_empty() && opening.starts_with(&found.from.to_lowercase()) {
            found.from.clear();
            found.mine = false;
        }
        found
    }

    /// Falls back to the page's text above the cursor when the model described the message instead
    /// of copying it: described, it read as the user's own, and the reply came back as the sender's.
    pub fn or_page(mut self, page: &str, answer: &str) -> Self {
        // A reaction like 👍 under the message isn't the message.
        let wordless = !self.message.chars().any(char::is_alphanumeric);
        if wordless || self.message.to_lowercase().contains("the user") {
            let tail = page_tail(page);
            self.message = if tail.is_empty() {
                answer.trim().to_string()
            } else {
                tail
            };
        }
        self
    }

    /// What the model read from the page's text, checked against that text. A small model wrote the
    /// user's answer in place of the message, copied the message into FROM, and gave the name a
    /// message greets ("Ka Yi, the report...") as its sender, making it the user's own.
    pub fn checked(mut self, page: &str, answer: &str, user_name: &str) -> Self {
        let wordless = !self.message.chars().any(char::is_alphanumeric);
        let described = self.message.to_lowercase().contains("the user");
        if wordless || described || !on_page(&self.message, page) {
            let tail = page_tail(page);
            self.message = if tail.is_empty() {
                answer.trim().to_string()
            } else {
                tail
            };
            // Not copied from the page, so who sent it can't be trusted either. The reply box can
            // still say who it goes to.
            self.from = email_sender("", page, user_name).unwrap_or_default();
            self.mine = false;
            return self;
        }
        if page_says_mine(&self.message, page, user_name) {
            self.mine = true;
            if same_person(&self.to, user_name) {
                self.to.clear();
            }
        } else if !user_name.trim().is_empty() {
            // With their name to go by, only the page says a message is theirs: a small model read
            // "Mei" above a message as "Me". Their name as its sender, with no line on the page
            // naming them as it, is the name the message is addressed to.
            if same_person(&self.from, user_name) {
                self.from.clear();
            }
            self.mine = false;
        }
        // A name from the list of chats at the top, far from the message, isn't who sent it.
        if !self.from.is_empty() && !sender_near(&self.from, &self.message, page) {
            self.from.clear();
        }
        // An email's sender is on the line with their address, which a small model left out, or
        // swapped for the team signing it off, so the reply greeted no one.
        if !self.mine {
            if let Some(sender) = email_sender(&self.message, page, user_name) {
                self.from = sender;
            }
        }
        self
    }

    /// A message the model stopped copying partway, as it's asked to so a reply starts sooner, read
    /// on from the page: from where the copy starts, up to the reply box. A legal notice at the end
    /// and the reply's own recipients are left out.
    pub fn read_on(mut self, page: &str) -> Self {
        let above = page.split(CURSOR_MARK).next().unwrap_or_default();
        let lines: Vec<String> = above
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect();
        let first: Vec<String> = words(&self.message).into_iter().take(3).collect();
        let Some(start) = lines
            .iter()
            .rposition(|line| first.len() >= 2 && has_run(&words(line), &first))
        else {
            return self;
        };
        let end = (start + 1..lines.len())
            .find(|&i| legal_notice(&lines[i]) || sender_on(&lines, i).is_some())
            .unwrap_or(lines.len());
        let read = prompts::truncate_chars(&lines[start..end].join("\n"), MOST_READ_ON_CHARS);
        if words(&read).len() > words(&self.message).len() {
            self.message = read;
        }
        self
    }

    /// The message as theirs, for a note that only thanks them: nobody thanks their own email. Read
    /// off a screenshot with the user's picture by the reply box, a small model took a bank's
    /// newsletter for the user's, and its reply chased the bank for an answer. Read as the user's,
    /// the other side was in TO.
    pub fn not_mine(mut self, user_name: &str) -> Self {
        if self.mine || same_person(&self.from, user_name) {
            self.mine = false;
            self.from = std::mem::take(&mut self.to);
        }
        self
    }

    /// Adds what the model saw in the message's pictures, unless it saw none.
    pub fn add_pictures(&mut self, answer: &str) {
        let answer = answer.trim();
        let plain = answer.trim_matches([' ', '.', '"', '\'']).to_lowercase();
        // "The message contains no picture, chart, or table. It is an email from...", as a small
        // model put it half the time, then went on to sum up the message.
        let first = plain.split(['.', '\n']).next().unwrap_or_default();
        let says_none = NO_PICTURES.iter().any(|none| first.contains(none));
        let none = matches!(plain.as_str(), "" | "none" | "no") || says_none;
        if !none && answer.chars().count() < MOST_PICTURE_CHARS {
            self.message
                .push_str(&format!("\n[Picture in the message: {answer}]"));
        }
    }
}

/// Whether two names are one person's, as in "Teoh Ka Yi" and "Ka Yi".
fn same_person(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    let shorter = a.chars().count().min(b.chars().count());
    if shorter >= 2 && (a.contains(&b) || b.contains(&a)) {
        return true;
    }
    // Run together, as in an address: "teohkayi@example.com" is Teoh Ka Yi's. Long enough not to
    // find "Ka Yi" in "Kayina".
    let letters = |name: &str| name.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    let (a, b) = (letters(&a), letters(&b));
    let shorter = a.chars().count().min(b.chars().count());
    shorter >= RUN_TOGETHER_NAME && (a.contains(&b) || b.contains(&a))
}

/// Whether the message was copied from the page above the cursor: as it is there, or with most of
/// its words there, as when only its main sentences were.
fn on_page(message: &str, page: &str) -> bool {
    let above = page.split(CURSOR_MARK).next().unwrap_or_default();
    // Pages use curly apostrophes where models write straight ones.
    let flat = |text: &str| {
        text.replace('’', "'")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let (above, message) = (flat(above), flat(message));
    if above.contains(&message) {
        return true;
    }
    let seen: HashSet<String> = words(&above).into_iter().collect();
    let message = words(&message);
    let there = message.iter().filter(|word| seen.contains(*word)).count();
    message.is_empty() || there as f32 >= message.len() as f32 * ON_PAGE_SHARE
}

/// Whether the page names the user as the message's sender: on a line of its own just above it, as
/// "me", "You" or their name, maybe with the time after it, or at the start of its line, as "You:".
fn page_says_mine(message: &str, page: &str, user_name: &str) -> bool {
    let first: Vec<String> = words(message).into_iter().take(3).collect();
    if first.len() < 2 {
        return false;
    }
    let forms: Vec<String> = prompts::name_forms(user_name)
        .iter()
        .map(|form| form.to_lowercase())
        .collect();
    let above = page.split(CURSOR_MARK).next().unwrap_or_default();
    let lines: Vec<&str> = above
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    // The message nearest the cursor, if its first words are on more than one line.
    let Some(at) = lines.iter().rposition(|line| has_run(&words(line), &first)) else {
        return false;
    };
    let line = lines[at].trim().to_lowercase();
    let labelled = |label: &str| {
        line.strip_prefix(label)
            .is_some_and(|rest| rest.trim_start().starts_with(':'))
    };
    if labelled("you") || labelled("me") || forms.iter().any(|form| labelled(form.as_str())) {
        return true;
    }
    lines[at.saturating_sub(4)..at]
        .iter()
        .rev()
        .find(|line| !between_sender_and_message(line))
        .is_some_and(|line| names_user(line, &forms))
}

/// Whether the sender's name is on the page near the message: a few lines above it, or in it, as a
/// sign-off. Further up, it's from a list of chats, as when a small model greeted "Mum" in a chat
/// with a friend.
fn sender_near(sender: &str, message: &str, page: &str) -> bool {
    let first: Vec<String> = words(message).into_iter().take(3).collect();
    let above = page.split(CURSOR_MARK).next().unwrap_or_default();
    let lines: Vec<&str> = above
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let at = lines
        .iter()
        .rposition(|line| first.len() >= 2 && has_run(&words(line), &first));
    let Some(at) = at else {
        return true;
    };
    let sender_lower = sender.to_lowercase();
    lines[at.saturating_sub(6)..].iter().any(|line| {
        let name = line.split('<').next().unwrap_or_default().trim();
        same_person(sender, name) || line.to_lowercase().contains(&sender_lower)
    })
}

/// Who sent an email, as "Name <address>", from the line with their address: above the email, as
/// Gmail's "Farahnadhia RAHMAN from.TP (DTSA)" over "<Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg>" or
/// Outlook's "From: Rachel Lim <rachel@partnerco.com>", or else under the reply box, among who the
/// reply goes to. Never the user, and nothing on a page with no addresses, like a chat's.
fn email_sender(message: &str, page: &str, user_name: &str) -> Option<String> {
    let lines = |text: &str| -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect()
    };
    let (above, below) = page.split_once(CURSOR_MARK).unwrap_or((page, ""));
    let (above, below) = (lines(above), lines(below));
    let first: Vec<String> = words(message).into_iter().take(3).collect();
    let start = above
        .iter()
        .rposition(|line| first.len() >= 2 && has_run(&words(line), &first));
    // The nearest address above the email is its sender's: when that's the user, the email is theirs.
    let header = start
        .and_then(|at| (at.saturating_sub(SENDER_LINES)..at).rev().find_map(|i| sender_on(&above, i)))
        .filter(|(name, _)| !same_person(name, user_name));
    // Or who the reply goes to, listed just above the box in Gmail, or under it.
    let recipient = |lines: &[String], range: Vec<usize>| {
        range
            .into_iter()
            .filter_map(|i| sender_on(lines, i))
            .find(|(name, _)| !same_person(name, user_name))
    };
    let over_box = (above.len().saturating_sub(RECIPIENT_LINES)..above.len()).rev();
    let under_box = 0..below.len().min(RECIPIENT_LINES);
    let (name, address) = header
        .or_else(|| recipient(&above, over_box.collect()))
        .or_else(|| recipient(&below, under_box.collect()))?;
    Some(format!("{name} <{address}>"))
}

/// A line of the legal notice under a company's emails: "This e-mail (including any attachments)
/// may contain confidential or legally privileged information...".
fn legal_notice(line: &str) -> bool {
    let line = line.to_lowercase();
    let legal = ["confidential", "privileged", "intended recipient", "unauthorised", "unauthorized"];
    let mail = ["e-mail", "email", "message", "attachment"];
    line.chars().count() > 60
        && legal.iter().any(|word| line.contains(word))
        && mail.iter().any(|word| line.contains(word))
}

/// The name and address on line `i`, when it has an address: "Rachel Lim <rachel@partnerco.com>",
/// or "<rachel@partnerco.com>" under "Rachel Lim". A line of who else it went to doesn't count.
fn sender_on(lines: &[String], i: usize) -> Option<(String, String)> {
    let line = lines[i].as_str();
    let address = line
        .split(|c: char| c.is_whitespace() || "<>()[],;:\"'".contains(c))
        .find(|word| {
            word.split_once('@').is_some_and(|(user, domain)| {
                !user.is_empty() && domain.contains('.') && !domain.ends_with('.')
            })
        })?;
    let lower = line.to_lowercase();
    let copied_in = ["to", "cc", "bcc"].iter().any(|field| {
        lower
            .strip_prefix(field)
            .is_some_and(|rest| rest.starts_with([':', ' ']))
    });
    if copied_in {
        return None;
    }
    let named = line.split(['<', '(']).next().unwrap_or_default().trim();
    let named = match named.get(..5) {
        Some(head) if head.eq_ignore_ascii_case("from:") => named[5..].trim(),
        _ => named,
    };
    let name = if named.is_empty() || named.contains('@') {
        lines.get(i.checked_sub(1)?)?.as_str()
    } else {
        named
    };
    let a_name = name.split_whitespace().count() <= MOST_NAME_WORDS
        && name.chars().any(char::is_alphabetic)
        && !name.contains('@')
        && !name.ends_with(['.', '!', '?', ':']);
    a_name.then(|| (name.to_string(), address.to_string()))
}

/// Lines between a sender's name and their message: a time or a date, "to careers" under the sender
/// of an email, or a short greeting line like "Hi Acme Careers team,". A name with the time after it,
/// "Divya Shah 10:42 AM", is the sender's line itself.
fn between_sender_and_message(line: &str) -> bool {
    let line = line.trim().to_lowercase();
    let bare = without_time(&line);
    let dated = bare.chars().any(|c| c.is_ascii_digit()) && bare.split_whitespace().count() <= 4;
    let greeting = ["hi", "hello", "hey", "dear"].iter().any(|greeting| {
        line.strip_prefix(greeting)
            .is_some_and(|rest| !rest.starts_with(char::is_alphanumeric))
    });
    bare.is_empty()
        || dated
        || line.starts_with(['•', '·'])
        || line.starts_with("to ")
        || (greeting && line.split_whitespace().count() <= 5)
}

/// Whether a sender line names the user: "me", "You", or their name or part of it, maybe with the
/// time or "(He/Him)" after it, as LinkedIn and Gmail show it.
fn names_user(line: &str, forms: &[String]) -> bool {
    let mut text = without_time(&line.to_lowercase());
    let bracket = text.rfind('(').filter(|_| text.ends_with(')'));
    if let Some(open) = bracket {
        text.truncate(open);
    }
    let text = text.trim().trim_matches(['•', '·', ':', ' ']);
    if matches!(text, "me" | "you") || forms.iter().any(|form| form == text) {
        return true;
    }
    text.split_whitespace().count() <= 8
        && forms.iter().any(|form| {
            text.strip_prefix(form.as_str()).is_some_and(|rest| {
                rest.is_empty() || rest.trim_start().starts_with(['•', '·', '(', '|', '-'])
            })
        })
}

/// A line without the time at its end: "Divya Shah 10:42 AM" is "Divya Shah".
fn without_time(line: &str) -> String {
    let mut parts: Vec<&str> = line.split_whitespace().collect();
    while let Some(last) = parts.last() {
        let last = last.to_lowercase();
        // "10:42", "9.30" or "1pm", but not "20" in "Oct 20".
        let (clock, suffixed) = match last.strip_suffix("am").or_else(|| last.strip_suffix("pm")) {
            Some(clock) => (clock, true),
            None => (last.as_str(), false),
        };
        let is_clock = clock.chars().any(|c| c.is_ascii_digit())
            && clock
                .chars()
                .all(|c| c.is_ascii_digit() || c == ':' || c == '.')
            && (suffixed || clock.contains([':', '.']));
        let is_mark = matches!(last.as_str(), "am" | "pm" | "•" | "·");
        if !(is_clock || is_mark) {
            break;
        }
        parts.pop();
    }
    parts.join(" ")
}

/// The kind of reply: the window title says it best, as a browser's is the site's, then the name of
/// the program, for apps whose title is only the chat's name, then what the model found.
pub fn kind_of(title: &str, app: &str, found: &str) -> MessageKind {
    let found = found.to_lowercase();
    let guess = if found.contains("email") {
        MessageKind::Email
    } else if found.contains("post") || found.contains("comment") {
        MessageKind::Comment
    } else if found.contains("chat") {
        MessageKind::Chat
    } else {
        MessageKind::Unknown
    };
    match (prompts::message_kind(title), prompts::message_kind(app)) {
        // LinkedIn and Facebook open chats over the feed, and a chat seen there is one.
        (MessageKind::Comment, _) if guess == MessageKind::Chat => MessageKind::Chat,
        (MessageKind::Unknown, MessageKind::Unknown) => guess,
        (MessageKind::Unknown, kind) | (kind, _) => kind,
    }
}

/// Turns the user's note into their reply to what was found, with the feeling their swearing showed.
pub fn write(
    p: &ReplyPrompts,
    kind: MessageKind,
    found: &Found,
    note: &Note,
    tone: &str,
    language: &Language,
    user_name: &str,
) -> Prompt {
    compose(p, kind, found, note, tone, language, user_name, false)
}

/// `write`, with the note given as the user's own draft of the reply. Asked again this way, a small
/// model that had told the user's news back to them as theirs mostly wrote it as the user's.
pub fn write_draft(
    p: &ReplyPrompts,
    kind: MessageKind,
    found: &Found,
    note: &Note,
    tone: &str,
    language: &Language,
    user_name: &str,
) -> Prompt {
    compose(p, kind, found, note, tone, language, user_name, true)
}

#[allow(clippy::too_many_arguments)]
fn compose(
    p: &ReplyPrompts,
    kind: MessageKind,
    found: &Found,
    note: &Note,
    tone: &str,
    language: &Language,
    user_name: &str,
    draft: bool,
) -> Prompt {
    let said = match (note.text.trim(), note.feeling) {
        ("", Feeling::Plain) => p.nothing_said.as_str(),
        ("", _) => p.only_swearing.as_str(),
        (text, _) => text,
    };
    let feeling = match (note.text.trim(), note.feeling) {
        // Only swearing: the note already says they're upset, and "be firm" on top of it had a
        // small model turn down a rejection that was already final.
        ("", _) | (_, Feeling::Plain) => "",
        (_, Feeling::Strong) => p.feeling_strong.as_str(),
        (_, Feeling::FedUp) => p.feeling_fed_up.as_str(),
    };
    // On a post, the side the note takes, spelled out: a small model otherwise wrote its own.
    let stance = match (kind, checks::stance(&note.text)) {
        (MessageKind::Comment, Some(checks::Stance::Agree)) => p.stance_agree.as_str(),
        (MessageKind::Comment, Some(checks::Stance::Disagree)) => p.stance_disagree.as_str(),
        (MessageKind::Comment, _) => "",
        // An angry no, spelled out too: a small model answered "Boy, I ain't going" with a
        // thank-you for having them, and asked again, still did. Said calmly, it got them right.
        _ if note.feeling != Feeling::Plain && checks::says_no(&note.text) => p.stance_no.as_str(),
        _ => "",
    };
    let feeling = format!("{feeling}{stance}");
    let format = match kind {
        MessageKind::Email => &p.format_email,
        MessageKind::Chat => &p.format_chat,
        MessageKind::Comment => &p.format_comment,
        MessageKind::Unknown => &p.format_unknown,
    };
    let label = match kind {
        MessageKind::Email => "EMAIL",
        MessageKind::Comment => "POST",
        MessageKind::Chat | MessageKind::Unknown => "MESSAGE",
    };
    // Read from the page, the sender comes with their address, which the model has no use for.
    let from = match found.from.split('<').next().unwrap_or_default().trim() {
        "" => String::new(),
        from => format!(" FROM {from}"),
    };
    // The last message is the user's own, by the model's reading or by their name in Settings. The
    // model can't tell them apart from the person they're talking to, as it names the user as
    // either, so their name decides when it's there.
    let to_user = same_person(&found.to, user_name);
    let mine = (found.mine || same_person(&found.from, user_name)) && !to_user;
    let to = match found.to.trim() {
        to if to.is_empty() || to_user => "them",
        to => to,
    };
    let name = match user_name.trim() {
        "" => p.name_unknown.clone(),
        name => fill(&p.name_known, &[("name", name)]),
    };
    Prompt {
        system: fill(
            &p.write_system,
            &[
                ("format", format.as_str()),
                ("tone", prompts::tone_line(tone)),
                ("language", language.rule().as_str()),
                ("name", name.as_str()),
            ],
        ),
        user: fill(
            if mine {
                &p.write_follow_up
            } else if draft {
                &p.write_user_draft
            } else {
                &p.write_user
            },
            &[
                ("kind", label),
                ("from", from.as_str()),
                ("to", to),
                ("message", found.message.trim()),
                ("note", said),
                ("feeling", feeling.as_str()),
            ],
        ),
        slot: Slot::Writing,
    }
}

/// The same prompt for another try, telling the model what to fix: `prompt` is `write`'s, or
/// `write_draft`'s for a reply that told the user's news back to them as theirs.
pub fn fix(p: &ReplyPrompts, prompt: &Prompt, problem: &checks::Problem) -> Prompt {
    use checks::Fix;
    let value = |key: &str| {
        problem
            .values
            .iter()
            .find(|(name, _)| *name == key)
            .map_or("", |(_, value)| value.as_str())
    };
    let wording = match problem.fix {
        Fix::Flip => &p.fix_flip,
        Fix::Example => &p.fix_example,
        Fix::TheirWords => &p.fix_their_words,
        Fix::Offer if !value("theirs").is_empty() => &p.fix_offer_swap,
        Fix::Offer => &p.fix_offer,
        Fix::SaidNo => &p.fix_said_no,
        Fix::SaidYes => &p.fix_said_yes,
        Fix::Side => &p.fix_side,
        Fix::Feelings => &p.fix_feelings,
        Fix::Lost => &p.fix_lost,
        Fix::Missing => &p.fix_missing,
        Fix::MadeUp => &p.fix_made_up,
        Fix::Link => &p.fix_link,
    };
    let values: Vec<(&str, &str)> = problem
        .values
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    Prompt {
        system: prompt.system.clone(),
        user: format!("{}{}", prompt.user, fill(wording, &values)),
        slot: prompt.slot,
    }
}

/// What a reply's first line opens with when it greets someone.
const GREETINGS: [&str; 7] = [
    "good morning",
    "good afternoon",
    "good evening",
    "hello",
    "hey",
    "dear",
    "hi",
];
/// Who "Hi there," and the like greet: no one in particular.
const NO_ONE: [&str; 5] = ["", "there", "all", "everyone", "team"];

/// An email greets its sender by name when they're a person. "Hi," and "Hi there," get their name, as
/// does a greeting to the team signing the email off, or to the user themself, and a reply with no
/// greeting gets one. A small model greeted no one, or left the greeting out when asked again.
pub fn greet(reply: &str, from: &str, user_name: &str) -> String {
    // The user's own name, as a small model gave for the sender of their own email, greets no one.
    let own = same_person(from.split('<').next().unwrap_or_default(), user_name);
    let name = checks::first_name(from).filter(|_| !own);
    let (first, rest) = reply.split_once('\n').unwrap_or((reply, ""));
    let Some((greeting, greeted, after)) = greeting(first.trim()) else {
        let to = name.map(|name| format!(" {name}")).unwrap_or_default();
        return format!("Hi{to},\n\n{}", reply.trim_start());
    };
    let rest = if rest.is_empty() {
        String::new()
    } else {
        format!("\n{rest}")
    };
    let to_user = same_person(greeted, user_name);
    let no_one = NO_ONE.contains(&greeted.to_lowercase().as_str())
        || to_user
        || checks::first_name(greeted).is_none();
    match name {
        Some(name) if no_one => format!("{greeting} {name},{after}{rest}"),
        None if to_user => format!("{greeting},{after}{rest}"),
        _ => reply.to_string(),
    }
}

/// "Hi Sarah," as ("Hi", "Sarah", ""), and "Hi, thanks for this." as ("Hi", "", " thanks for this."):
/// the greeting, who it greets and the rest of the line. Nothing for a line that greets no one.
fn greeting(line: &str) -> Option<(&str, &str, &str)> {
    let size = GREETINGS
        .iter()
        .find(|greeting| {
            line.get(..greeting.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(greeting))
                && !line[greeting.len()..].starts_with(char::is_alphanumeric)
        })?
        .len();
    let (greeting, rest) = line.split_at(size);
    let (greeted, after) = rest.split_once(',').unwrap_or((rest, ""));
    Some((greeting, greeted.trim().trim_end_matches(['!', '.']), after))
}

/// The same prompt for another try after a refusal, telling the model plainly that the note is fine
/// to write. The last try also makes it a rewrite of the user's words.
pub fn insist(p: &ReplyPrompts, prompt: &Prompt, last_try: bool) -> Prompt {
    let system = if last_try {
        format!("{REWRITE_ONLY}{}", prompt.system)
    } else {
        prompt.system.clone()
    };
    Prompt {
        system,
        user: format!("{}{}", prompt.user, p.insist),
        slot: prompt.slot,
    }
}

/// How many of their words in a row make a line of the reply theirs rather than the user's.
const ECHO_WORDS: usize = 8;
/// A shorter reply is theirs when it's all their words and at least this many, like their own
/// question, "Who's bringing drinks for Saturday?", asked back to them.
const SHORT_ECHO_WORDS: usize = 4;

/// A run of their message's words the reply hands back to them, like a rejection's "we wish you all
/// the best in your studies", which a small model signed off with however it was told not to. Words
/// the user's note gave don't count.
pub fn echo(reply: &str, message: &str, note: &str) -> Option<String> {
    let (reply, message, note) = (words(reply), words(message), words(note));
    let size = ECHO_WORDS.min(reply.len()).max(SHORT_ECHO_WORDS);
    reply
        .windows(size)
        .find(|run| has_run(&message, run) && !has_run(&note, run))
        .map(|run| run.join(" "))
}

/// Words a rough spoken note has and a written reply doesn't.
const ROUGH: [&str; 15] = [
    "nah", "bro", "brah", "bruh", "dude", "yo", "ya", "u", "gonna", "wanna", "lol", "ugh", "im",
    "cant", "dont",
];

/// Whether the reply is the user's rough note sent back nearly as it was, slang and all, as a small
/// model did with "nah bro im broke too". A note that's already fine to send can come back as it is.
pub fn echoes_note(reply: &str, note: &str) -> bool {
    let (reply, note) = (words(reply), words(note));
    let rough = reply.iter().any(|word| ROUGH.contains(&word.as_str()));
    if note.len() < 2 || reply.is_empty() || !rough {
        return false;
    }
    let from_note = reply.iter().filter(|word| note.contains(word)).count();
    reply.len() <= note.len() + 2 && from_note * 10 >= reply.len() * 7
}

/// The same prompt for another try, asking for the reply rather than the note back.
pub fn without_note_echo(p: &ReplyPrompts, prompt: &Prompt) -> Prompt {
    Prompt {
        system: prompt.system.clone(),
        user: format!("{}{}", prompt.user, p.no_note_echo),
        slot: prompt.slot,
    }
}

/// The words in some text, in lower case, keeping apostrophes so "I'm" stays one word.
fn words(text: &str) -> Vec<String> {
    // Pages use curly apostrophes where models write straight ones.
    text.to_lowercase()
        .replace('\u{2019}', "'")
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .map(String::from)
        .collect()
}

fn has_run(text: &[String], run: &[String]) -> bool {
    !run.is_empty() && text.windows(run.len()).any(|seen| seen == run)
}

/// A line cut into sentences, each keeping its closing mark and the space before it.
fn sentences(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = line.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let ends = chars.peek().is_none_or(|(_, next)| next.is_whitespace());
        if matches!(c, '.' | '!' | '?') && ends {
            let end = at + c.len_utf8();
            out.push(&line[start..end]);
            start = end;
        }
    }
    if start < line.len() {
        out.push(&line[start..]);
    }
    out
}

/// Takes out the sentence holding `copied`, for when asking again copied it too. Left as it was if
/// that would leave only a greeting and a sign-off, with or without the user's name under it.
pub fn drop_echo(reply: &str, copied: &str, user_name: &str) -> String {
    let copied = words(copied);
    let mut kept: Vec<String> = Vec::new();
    for line in reply.lines() {
        let rest: String = sentences(line)
            .into_iter()
            .filter(|sentence| !has_run(&words(sentence), &copied))
            .collect();
        let emptied = rest.trim().is_empty() && !line.trim().is_empty();
        let blank_again = rest.trim().is_empty() && kept.last().is_some_and(|l| l.is_empty());
        if !emptied && !blank_again {
            kept.push(rest.trim().to_string());
        }
    }
    let out = kept.join("\n").trim().to_string();
    // "Hi Sarah," and "Best regards," end in commas, and aren't a reply on their own, nor is a name.
    let body: Vec<&str> = out
        .lines()
        .filter(|line| !line.trim_end().ends_with(',') && !same_person(line, user_name))
        .collect();
    if words(&body.join(" ")).len() < 2 {
        reply.to_string()
    } else {
        out
    }
}

/// How sentences start that ask them for something, tell them what to do, or promise something.
const MORE_THAN_THANKS: [&str; 18] = [
    "please",
    "kindly",
    "feel free",
    "could you",
    "can you",
    "would you",
    "let me know",
    "you should",
    "you must",
    "make sure",
    "remember",
    "i will",
    "i'll",
    "i would like",
    "i'd like",
    "i want to",
    "i am not sure",
    "i'm not sure",
];
/// Words that thank, which keep a sentence that doesn't speak for them.
const THANKING: [&str; 6] = ["thank", "thanks", "appreciate", "appreciated", "grateful", "cheers"];
/// How sentences start that speak for them, which go even when they thank: a small model thanked
/// a bank with "We appreciate you sharing your details with us".
const THEIR_VOICE: [&str; 6] = ["we", "we're", "we've", "we'll", "we'd", "our"];

/// A reply to a note that only thanks them, cut back to thanks. A small model went on to warn a
/// bank that its own newsletter was a scam, and to ask it to call. Sentences that ask them for
/// something, tell them what to do, promise something or talk about `them` by name go, unless they
/// thank them too; "I look forward to attending" stays. Left as it was if nothing would be left.
pub fn just_thanks(reply: &str, them: &str, user_name: &str) -> String {
    let name = words(them.split(['<', '(']).next().unwrap_or_default());
    let more = |sentence: &str| {
        let said = words(sentence);
        let starts = |start: &[String]| !start.is_empty() && said.starts_with(start);
        let thanks = said.iter().any(|word| THANKING.contains(&word.as_str()));
        let theirs = said
            .first()
            .is_some_and(|word| THEIR_VOICE.contains(&word.as_str()));
        let more = starts(&name) || MORE_THAN_THANKS.iter().any(|lead| starts(&words(lead)));
        theirs || (more && !thanks)
    };
    let mut kept: Vec<String> = Vec::new();
    for line in reply.lines() {
        let rest = if line.trim_end().ends_with(',') || same_person(line, user_name) {
            line.to_string()
        } else {
            sentences(line)
                .into_iter()
                .filter(|sentence| !more(sentence))
                .collect()
        };
        let emptied = rest.trim().is_empty() && !line.trim().is_empty();
        let blank_again = rest.trim().is_empty() && kept.last().is_some_and(|l| l.is_empty());
        if !emptied && !blank_again {
            kept.push(rest.trim().to_string());
        }
    }
    let out = kept.join("\n").trim().to_string();
    let body: Vec<&str> = out
        .lines()
        .filter(|line| !line.trim_end().ends_with(',') && !same_person(line, user_name))
        .collect();
    if words(&body.join(" ")).len() < 2 {
        reply.to_string()
    } else {
        out
    }
}

/// Words a sentence ends on when all it did was lead to a link: "Please find the link below".
const LEADS_TO_LINK: [&str; 6] = ["below", "here", "link", "following", "url", "website"];

/// Takes out their link, which asking again didn't: from its sentence, with the word that led to it,
/// so "I've shared my portfolio at github.com/kayi and would like an update" keeps the rest. A
/// sentence that only led to it, like "Please find the link below:", goes, and so does one on the
/// line above a link on its own. Left as it was if that would leave nothing to send.
pub fn without_link(reply: &str, link: &str, user_name: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    for line in reply.lines() {
        if !line.contains(link) {
            kept.push(line.to_string());
            continue;
        }
        let rest: String = sentences(line)
            .into_iter()
            .filter_map(|sentence| {
                if !sentence.contains(link) {
                    return Some(sentence.to_string());
                }
                let cut = cut_link(sentence, link);
                let left = words(&cut);
                let leads = left
                    .last()
                    .is_some_and(|word| LEADS_TO_LINK.contains(&word.as_str()));
                (left.len() >= 4 && !leads).then_some(cut)
            })
            .collect();
        if !rest.trim().is_empty() {
            kept.push(rest.trim().to_string());
            continue;
        }
        if let Some(last) = kept.last_mut().filter(|last| last.trim_end().ends_with(':')) {
            let led: String = sentences(last)
                .into_iter()
                .filter(|sentence| !sentence.trim_end().ends_with(':'))
                .collect();
            *last = led.trim().to_string();
        }
    }
    let mut out: Vec<&str> = Vec::new();
    for line in &kept {
        if !(line.trim().is_empty() && out.last().is_some_and(|last| last.trim().is_empty())) {
            out.push(line);
        }
    }
    let out = out.join("\n").trim().to_string();
    // "Hi Sarah," and "Best regards," end in commas, and aren't a reply on their own, nor is a name.
    let body: Vec<&str> = out
        .lines()
        .filter(|line| !line.trim_end().ends_with(',') && !same_person(line, user_name))
        .collect();
    if words(&body.join(" ")).len() < 2 {
        reply.to_string()
    } else {
        out
    }
}

/// `sentence` without `link` and the word that led to it: "my portfolio at github.com/kayi and"
/// gives "my portfolio and".
fn cut_link(sentence: &str, link: &str) -> String {
    let Some(at) = sentence.find(link) else {
        return sentence.to_string();
    };
    let before = sentence[..at].trim_end().trim_end_matches(['(', '<', ':']).trim_end();
    let lower = before.to_lowercase();
    let before = [" at", " via", " here", " on"]
        .iter()
        .find(|lead| lower.ends_with(*lead))
        .map_or(before, |lead| &before[..before.len() - lead.len()]);
    let after = sentence[at + link.len()..].trim_start_matches(['/', ')', '>']);
    format!("{before}{after}")
}

/// The same prompt for another try, telling the model which of their words it copied.
pub fn without_echo(p: &ReplyPrompts, prompt: &Prompt, copied: &str) -> Prompt {
    Prompt {
        system: prompt.system.clone(),
        user: format!("{}{}", prompt.user, fill(&p.no_echo, &[("copied", copied)])),
        slot: prompt.slot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wording() -> ReplyPrompts {
        ReplyPrompts::default()
    }

    fn note(text: &str, feeling: Feeling) -> Note {
        Note {
            text: text.into(),
            feeling,
        }
    }

    #[test]
    fn the_built_in_wording_has_every_part() {
        let p = wording();
        assert!(p.find_system.contains(CURSOR_MARK));
        assert!(p.find_user.contains("{page}"));
        assert!(p.write_system.contains("{format}") && p.write_system.contains("{name}"));
        assert!(p.write_user.contains("{message}") && p.write_user.contains("{note}"));
        assert!(p.write_user.contains("{feeling}"));
    }

    #[test]
    fn a_copy_in_the_data_folder_replaces_only_its_keys() {
        let dir = std::env::temp_dir().join(format!("wisperchad-reply-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!ReplyPrompts::load(&dir).1);
        std::fs::write(
            dir.join(PROMPTS_FILE),
            r#"{"format_chat": "- Short.", "nope": "x"}"#,
        )
        .unwrap();
        let (p, custom) = ReplyPrompts::load(&dir);
        assert!(custom);
        assert_eq!(p.format_chat, "- Short.");
        assert_eq!(p.format_email, wording().format_email);
        std::fs::write(dir.join(PROMPTS_FILE), "{ not json").unwrap();
        let (p, custom) = ReplyPrompts::load(&dir);
        assert!(!custom);
        assert_eq!(p.format_chat, wording().format_chat);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn filling_in_never_reads_what_was_put_in_as_a_key() {
        assert_eq!(
            fill("{a} and {b} and {c}", &[("a", "{b}"), ("b", "two")]),
            "{b} and two and {c}"
        );
        assert_eq!(fill("no keys {", &[]), "no keys {");
    }

    #[test]
    fn only_the_page_above_the_cursor_is_read() {
        let page = format!("menu\nTheir post\n{CURSOR_MARK}\nThe next post");
        let p = find(&wording(), " Feed | LinkedIn ", &page);
        assert!(p.user.contains("Their post"));
        assert!(!p.user.contains("The next post"));
        assert!(p.user.contains("WINDOW TITLE: Feed | LinkedIn"));
        assert!(has_page_text(&"x".repeat(ENOUGH_TEXT)));
        assert!(!has_page_text(&format!("Chats\n{CURSOR_MARK}")));
    }

    #[test]
    fn what_was_found_is_read_back() {
        let found = Found::parse(
            "**FROM:** Mr Lee\nKIND: chat\nMESSAGE: Redo the report.\nBy 8am, please.",
        );
        assert_eq!(found.from, "Mr Lee");
        assert_eq!(found.kind, "chat");
        assert_eq!(found.message, "Redo the report.\nBy 8am, please.");
        assert!(!found.mine);
        assert_eq!(Found::parse("FROM: unknown\nMESSAGE: hi").from, "");
        let sent = Found::parse(
            "FROM: Teoh Ka Yi\nTO: Grace Wong\nMINE: yes\nKIND: chat\nMESSAGE: Hey Grace!",
        );
        assert_eq!((sent.to.as_str(), sent.mine), ("Grace Wong", true));
        assert!(Found::parse("FROM: me\nTO: Acme\nMINE: no\nMESSAGE: Any update?").mine);
        // The person they're talking to never wrote the user's message.
        assert!(!Found::parse("FROM: Divya Shah\nTO: Divya\nMINE: yes\nMESSAGE: Hi").mine);
        // A name the message opens with is who it's for, not who wrote it.
        assert_eq!(
            Found::parse("FROM: Ka Yi\nMESSAGE: Ka Yi, the report...").from,
            ""
        );
        let greeted = Found::parse("FROM: Ka Yi\nMINE: yes\nMESSAGE: Hi Ka Yi, are you free?");
        assert_eq!((greeted.from.as_str(), greeted.mine), ("", false));
        // A message to "me" is never the user's own.
        let to_me = Found::parse("FROM: Acme\nTO: me\nMINE: yes\nMESSAGE: We regret...");
        assert_eq!((to_me.to.as_str(), to_me.mine), ("", false));
    }

    #[test]
    fn a_described_message_gives_way_to_the_page() {
        let page = format!("Inbox\nWe regret to inform you...\n{CURSOR_MARK}");
        let found = Found::parse("FROM: Acme\nMESSAGE: The user is replying to a rejection.");
        assert_eq!(
            found.or_page(&page, "x").message,
            "Inbox\nWe regret to inform you..."
        );
        let reaction = Found::parse("FROM: unknown\nMESSAGE: 👍");
        assert_eq!(
            reaction.or_page(&page, "x").message,
            "Inbox\nWe regret to inform you..."
        );
        let copied = Found::parse("FROM: Acme\nMESSAGE: We regret to inform you.");
        assert_eq!(
            copied.or_page(&page, "x").message,
            "We regret to inform you."
        );
    }

    #[test]
    fn an_email_greets_the_sender_by_name() {
        let from = "Farahnadhia RAHMAN from.TP (DTSA) <Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg>";
        let user = "Teoh Ka Yi";
        let greet = |reply: &str, from: &str| greet(reply, from, user);
        assert_eq!(
            greet("Hi,\n\nThanks, see you there.\n\nBest regards,", from),
            "Hi Farahnadhia,\n\nThanks, see you there.\n\nBest regards,"
        );
        assert_eq!(
            greet("Unfortunately, my laptop died.", "Dr Tan Wei Ming"),
            "Hi Dr Tan,\n\nUnfortunately, my laptop died."
        );
        // A greeting to no one in particular, to the team signing it off or to the user gets their name.
        assert_eq!(
            greet("Hi DTSA Team,\n\nThanks.", from),
            "Hi Farahnadhia,\n\nThanks."
        );
        assert_eq!(greet("Dear Ka Yi,\n\nThanks.", from), "Dear Farahnadhia,\n\nThanks.");
        assert_eq!(greet("Hello there!\nThanks.", from), "Hello Farahnadhia,\nThanks.");
        assert_eq!(
            greet("Dear Sir/Madam,\n\nThanks.", from),
            "Dear Farahnadhia,\n\nThanks."
        );
        assert_eq!(
            greet("Hi, thanks for the details.", from),
            "Hi Farahnadhia, thanks for the details."
        );
        // A team gets a plain "Hi,", and a greeting with a name stays as it is.
        assert_eq!(
            greet("Thanks for letting me know.", "Early Careers AP"),
            "Hi,\n\nThanks for letting me know."
        );
        assert_eq!(greet("Hi,\n\nThanks.", "Finance Team"), "Hi,\n\nThanks.");
        assert_eq!(greet("Hi Ka Yi,\n\nThanks.", "Finance Team"), "Hi,\n\nThanks.");
        // Their own name as the sender greets no one: the team they wrote to stays.
        assert_eq!(
            greet("Hi Acme Careers team,\n\nThanks.", "Ka Yi"),
            "Hi Acme Careers team,\n\nThanks."
        );
        assert_eq!(greet("Hi Sarah,\n\nThanks.", from), "Hi Sarah,\n\nThanks.");
        assert_eq!(
            greet("Hello and thank you for the email.", from),
            "Hello and thank you for the email."
        );
    }

    #[test]
    fn a_link_comes_out_of_its_sentence() {
        let link = "github.com/kayi";
        let reply = "Hi Acme Careers team,\n\nI've shared my portfolio at github.com/kayi and would \
appreciate any updates.\n\nBest regards,\nTeoh Ka Yi";
        assert_eq!(
            without_link(reply, link, "Teoh Ka Yi"),
            "Hi Acme Careers team,\n\nI've shared my portfolio and would appreciate any updates.\n\n\
Best regards,\nTeoh Ka Yi"
        );
        let reply = "Hi,\n\nMy portfolio is attached for your reference: github.com/kayi.\n\nThanks,";
        assert_eq!(
            without_link(reply, link, ""),
            "Hi,\n\nMy portfolio is attached for your reference.\n\nThanks,"
        );
        // A sentence that only led to it goes; if nothing else is left, the reply stays as it was.
        let reply =
            "Hi,\n\nThanks for the details. Submit your idea here: https://go.gov.sg/form\n\nBest regards,";
        assert_eq!(
            without_link(reply, "https://go.gov.sg/form", ""),
            "Hi,\n\nThanks for the details.\n\nBest regards,"
        );
        let only = "Hi,\n\nhttps://go.gov.sg/form\n\nBest regards,";
        assert_eq!(without_link(only, "https://go.gov.sg/form", ""), only);
    }

    #[test]
    fn their_link_comes_out_with_what_led_to_it() {
        let reply = "Hi,\n\nThank you for confirming my registration.\n\nRegarding the idea submission, I am \
ready to share my thoughts. Please find the link below:\nhttps://go.gov.sg/demoday101prepform\n\n\
Best regards,\nTeoh Ka Yi";
        assert_eq!(
            without_link(reply, "https://go.gov.sg/demoday101prepform", "Teoh Ka Yi"),
            "Hi,\n\nThank you for confirming my registration.\n\nRegarding the idea submission, I am ready \
to share my thoughts.\n\nBest regards,\nTeoh Ka Yi"
        );
    }

    /// The DTSA email as Gmail in Brave shares it, cut short: icons stand in as lines of their own.
    fn gmail_dtsa() -> String {
        let icon = "\u{fffc}";
        format!(
            "Inbox\n{icon}\nFarahnadhia RAHMAN from.TP (DTSA) Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg\n{icon}\n\
Nov 5, 2026, 10:16 AM (2 days ago)\n{icon}\n{icon}\n{icon}\n{icon}\n{icon}\n{icon}\n{icon}\n{icon}\n{icon}\n\
to me\nShow details\n{icon}\n{icon}\n{icon}\n{icon}\nHi Ka Yi,\nThank you for registering for DTSA's \
From Student to Founder: Building Your First Company workshop!\nWe are pleased to confirm your participation.\n\
Event Details\n\u{2022} Date: Wednesday, 11 November 2026\nBest regards,\nDTSA Team\n{icon}\n\
This e-mail (including any attachments) may contain confidential or legally privileged information. Any \
unauthorised use, retention,\n{icon}\n{icon}\nFarahnadhia RAHMAN from.TP (DTSA) \
(Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg)\nPop out reply\n{icon}\n{CURSOR_MARK}\n\n{icon}\nSend"
        )
    }

    #[test]
    fn icons_dont_push_an_emails_sender_out_of_reach() {
        let page = tidy_page(&gmail_dtsa());
        assert!(!page.contains('\u{fffc}') && !page.contains("\n\n"));
        let answer = "FROM: Farahnadhia RAHMAN from.TP (DTSA) Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg\n\
TO: Ka Yi\nMINE: no\nKIND: email\nMESSAGE: Hi Ka Yi,\nThank you for registering for DTSA's From Student";
        let found = Found::parse(answer).checked(&page, answer, "Teoh Ka Yi");
        assert_eq!(checks::first_name(&found.from).as_deref(), Some("Farahnadhia"));
        // Even untidied, the reply's own recipient above the box names them.
        let found = Found::parse(answer).checked(&gmail_dtsa(), answer, "Teoh Ka Yi");
        assert_eq!(checks::first_name(&found.from).as_deref(), Some("Farahnadhia"));
        // And a message the model didn't copy from the page still gets them, from there.
        let made_up = "FROM: DTSA\nMESSAGE: The user registered for a workshop.";
        let found = Found::parse(made_up).checked(&page, made_up, "Teoh Ka Yi");
        assert_eq!(checks::first_name(&found.from).as_deref(), Some("Farahnadhia"));
    }

    #[test]
    fn a_message_cut_short_is_read_on_from_the_page() {
        let page = tidy_page(&gmail_dtsa());
        let cut = "FROM: Farahnadhia RAHMAN from.TP (DTSA)\nMESSAGE: Hi Ka Yi,\nThank you for registering for DTSA's From";
        let found = Found::parse(cut).checked(&page, cut, "Teoh Ka Yi").read_on(&page);
        assert!(found.message.starts_with("Hi Ka Yi,\nThank you for registering"));
        assert!(found.message.ends_with("Best regards,\nDTSA Team"), "{}", found.message);
        // A message it couldn't find on the page stays as it was.
        let lost = Found::parse("MESSAGE: Something else entirely").read_on(&page);
        assert_eq!(lost.message, "Something else entirely");
    }

    #[test]
    fn an_emails_sender_is_read_off_the_page() {
        let user = "Teoh Ka Yi";
        let message = "Hi Ka Yi,\nThank you for registering for the workshop!";
        // Gmail: the name, then the address, then the date, a few buttons and "to me".
        let gmail = format!(
            "Inbox\nSummarize this email\nFarahnadhia RAHMAN from.TP (DTSA)\n\
<Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg>\nUnsubscribe\nNov 5, 2026, 10:16 AM (2 days ago)\nNot starred\n\
React\nReply\nMore\nto me\nShow details\nHi Ka Yi,\nThank you for registering for the workshop!\n\
Best regards,\nDTSA Team\n@DTSAsg\n{CURSOR_MARK}\nFarahnadhia RAHMAN from.TP (DTSA) \
(Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg)"
        );
        let sender = "Farahnadhia RAHMAN from.TP (DTSA) <Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg>";
        assert_eq!(email_sender(message, &gmail, user).as_deref(), Some(sender));
        // The model took the team signing it off for the sender, and the page puts it right.
        let answer = format!("FROM: DTSA Team\nTO: Ka Yi\nMINE: no\nKIND: email\nMESSAGE: {message}");
        let found = Found::parse(&answer).checked(&gmail, &answer, user);
        assert_eq!(found.from, sender);
        assert_eq!(checks::first_name(&found.from).as_deref(), Some("Farahnadhia"));
        // Outlook, with who else it went to under the sender.
        let outlook = format!(
            "From: Rachel Lim <rachel@partnerco.com>\nTo: Teoh Ka Yi <teohkayi@example.com>\n\
Cc: Ops <ops@partnerco.com>\nHi Ka Yi,\nThank you for registering for the workshop!\n{CURSOR_MARK}"
        );
        assert_eq!(
            email_sender(message, &outlook, user).as_deref(),
            Some("Rachel Lim <rachel@partnerco.com>")
        );
        // The user's own email, answered from under the box, goes to who's listed there.
        let own = format!(
            "Teoh Ka Yi <teohkayi@example.com>\nto Rachel\nHi Ka Yi,\nThank you for registering for \
the workshop!\n{CURSOR_MARK}\nRachel Lim <rachel@partnerco.com>"
        );
        assert_eq!(
            email_sender(message, &own, user).as_deref(),
            Some("Rachel Lim <rachel@partnerco.com>")
        );
        // A chat has no addresses, so there's nothing to read.
        let chat = format!("Chats\nDivya Shah\nHi Ka Yi, are you free?\n{CURSOR_MARK}");
        assert_eq!(email_sender("Hi Ka Yi, are you free?", &chat, user), None);
    }

    #[test]
    fn a_message_the_model_wrote_gives_way_to_the_page() {
        let page = format!(
            "Messaging\nDivya Shah\nDivya Shah 10:42 AM\nHi Ka Yi, I came across your profile and \
think you'd be a great fit for our internship. Are you open to a quick chat this week?\n\
{CURSOR_MARK}\nSend"
        );
        // It wrote the user's answer as the message, and the user as its sender.
        let answer = "FROM: Ka Yi\nTO: Divya Shah\nMINE: yes\nMESSAGE: Hi Divya, thanks for \
reaching out! I am definitely open to a quick chat this week.";
        let found = Found::parse(answer).checked(&page, answer, "Teoh Ka Yi");
        assert!(found.message.contains("great fit") && found.message.starts_with("Messaging"));
        assert_eq!((found.from.as_str(), found.mine), ("", false));
        // Copied, if only its main sentences, it stays.
        let answer = "FROM: Divya Shah\nMESSAGE: I came across your profile and think you'd be a \
great fit for our internship.";
        let found = Found::parse(answer).checked(&page, answer, "Teoh Ka Yi");
        assert_eq!(found.from, "Divya Shah");
        assert!(found.message.starts_with("I came across"));
        // A message copied into FROM is no one's name.
        let friend = Found::parse(
            "FROM: yo you coming for dinner tonight? 8pm at the usual place\nMESSAGE: yo",
        );
        assert_eq!(friend.from, "");
    }

    #[test]
    fn the_page_says_whose_message_it_is() {
        let user = "Teoh Ka Yi";
        let read =
            |answer: &str, page: &str, user: &str| Found::parse(answer).checked(page, answer, user);
        // Their name above it, with the time, makes it theirs, whatever the model said.
        let grace = format!(
            "Messaging\nGrace Wong\nSEP 22\nTeoh Ka Yi\n• 10:46 AM\nHey Grace! Thanks for \
connecting!\nDoes NOVA DAO take interns?\n{CURSOR_MARK}"
        );
        let found = read(
            "FROM: Grace Wong\nTO: Teoh Ka Yi\nMINE: no\nMESSAGE: Hey Grace! Thanks for connecting!",
            &grace,
            user,
        );
        assert_eq!((found.mine, found.to.as_str()), (true, ""));
        // So does "me" above an email, past its date and "to careers".
        let sent = format!(
            "Internship application\nme\nto careers\nOct 20\nHi Acme Careers team,\nI applied for \
the internship on 20 October.\nBest regards,\nKa Yi\n{CURSOR_MARK}"
        );
        let found = read(
            "FROM: me@gmail.com\nTO: careers\nMINE: no\nMESSAGE: I applied for the internship on 20 \
October.",
            &sent,
            user,
        );
        assert!(found.mine);
        // And "You:" before it on its line.
        let chat = format!(
            "David Tan\nDavid Tan: dinner tonight at 8?\n6:02 PM\nYou: yep! same place as last \
time?\n6:05 PM\n{CURSOR_MARK}"
        );
        let found = read(
            "FROM: David Tan\nMINE: no\nMESSAGE: yep! same place as last time?",
            &chat,
            user,
        );
        assert!(found.mine);
        // Their name as the sender, with nothing on the page saying so, is who it's addressed to.
        let boss = format!(
            "Chats\nMum\nRyan\nMr Lee\nKa Yi, the report you sent is not good enough. Redo it by \
8am tomorrow.\n9:14 PM\n{CURSOR_MARK}"
        );
        let found = read(
            "FROM: Ka Yi\nTO: Mr Lee\nMINE: yes\nMESSAGE: the report you sent is not good enough.",
            &boss,
            user,
        );
        assert_eq!((found.from.as_str(), found.mine), ("", false));
        // A name from the chat list taken for the sender doesn't make it theirs either.
        let friend =
            format!("Chats\nMum\nDavid Lim\nyo you coming for dinner tonight?\n{CURSOR_MARK}");
        let answer = "FROM: Mum\nTO: Ryan\nMINE: yes\nMESSAGE: yo you coming for dinner tonight?";
        assert!(!read(answer, &friend, user).mine);
        // Without their name, the model's word is all there is.
        assert!(read(answer, &friend, "").mine);
        let forms = ["teoh ka yi".to_string()];
        assert!(names_user("Teoh Ka Yi (He/Him) • 10:46 AM", &forms));
        assert!(!names_user("Teoh Ka Yi, are you free?", &forms));
        assert_eq!(without_time("Divya Shah 10:42 AM"), "Divya Shah");
        assert_eq!(without_time("Oct 20"), "Oct 20");
    }

    #[test]
    fn pictures_are_added_unless_there_are_none() {
        let mut found = Found::copied("See the chart.");
        found.add_pictures("None.");
        assert_eq!(found.message, "See the chart.");
        found.add_pictures("The message contains no picture, chart, or table. It is an email.");
        assert_eq!(found.message, "See the chart.");
        found.add_pictures("Option B costs $640.");
        assert!(found
            .message
            .ends_with("[Picture in the message: Option B costs $640.]"));
    }

    #[test]
    fn the_title_decides_the_kind_then_the_program_then_the_model() {
        use MessageKind::*;
        assert_eq!(kind_of("Inbox - Gmail", "Google Chrome", "chat"), Email);
        assert_eq!(kind_of("Feed | LinkedIn", "Brave Browser", "post"), Comment);
        // A chat opened over the feed is a chat.
        assert_eq!(kind_of("Feed | LinkedIn", "Brave Browser", "chat"), Chat);
        // A title that's only the chat's or email's name leaves it to the program.
        assert_eq!(kind_of("Anna Teo", "Microsoft Teams", "email"), Chat);
        assert_eq!(
            kind_of("Re: Invoice 1042", "Microsoft Outlook", "chat"),
            Email
        );
        assert_eq!(kind_of("Mum", "Weixin", "email"), Chat);
        // The title still wins: Outlook can show a WhatsApp receipt.
        assert_eq!(
            kind_of(
                "Your WhatsApp receipt - Outlook",
                "Microsoft Outlook",
                "chat"
            ),
            Email
        );
        assert_eq!(kind_of("Notepad", "Notepad", "post"), Comment);
        assert_eq!(kind_of("Notepad", "Notepad", "an email"), Email);
        assert_eq!(kind_of("", "", "?"), Unknown);
    }

    #[test]
    fn the_reply_gets_its_form_tone_language_and_name() {
        let found = Found {
            from: "Divya".into(),
            kind: "chat".into(),
            message: "Are you free?".into(),
            ..Found::default()
        };
        let english = Language::Named("English".into());
        let p = write(
            &wording(),
            MessageKind::Comment,
            &found,
            &note(" yes ", Feeling::Plain),
            "casual",
            &english,
            "",
        );
        assert!(p.system.contains(&wording().format_comment));
        assert!(p.system.contains("Casual"));
        assert!(p.system.contains("Write the whole reply in English"));
        assert!(p.system.contains(&wording().name_unknown));
        assert!(p.user.contains("THEIR POST FROM Divya"));
        assert!(p.user.contains("Are you free?") && p.user.contains("yes"));
        let p = write(
            &wording(),
            MessageKind::Email,
            &found,
            &note("yes", Feeling::Plain),
            "professional",
            &english,
            "Ka Yi",
        );
        assert!(p.system.contains("never greet, thank or address Ka Yi"));
        assert!(p.user.contains("THEIR EMAIL"));
        assert_eq!(p.slot, Slot::Writing);
        assert_eq!(find(&wording(), "Inbox", "hi").slot, Slot::Reading);
    }

    #[test]
    fn an_angry_no_is_spelled_out() {
        let found = Found {
            from: "Farahnadhia".into(),
            message: "We are pleased to confirm your participation.".into(),
            ..Found::default()
        };
        let english = Language::Named("English".into());
        let ask = |said: &str, feeling| {
            let note = note(said, feeling);
            let p = write(&wording(), MessageKind::Email, &found, &note, "professional", &english, "");
            p.user.contains(&wording().stance_no)
        };
        assert!(ask("Boy, I ain't going boy.", Feeling::FedUp));
        assert!(ask("Boy what boy I ain't good boy no man", Feeling::FedUp));
        // Said calmly, or keen, or a yes, it's left to the note.
        assert!(!ask("I can't make it, sorry", Feeling::Plain));
        assert!(!ask("I can't wait", Feeling::Strong));
        assert!(!ask("Okay thanks", Feeling::FedUp));
    }

    #[test]
    fn the_users_own_last_message_gets_a_follow_up() {
        let p = wording();
        let english = Language::Named("English".into());
        let sent = Found::parse(
            "FROM: Teoh Ka Yi\nTO: Grace Wong\nMINE: yes\nKIND: chat\nMESSAGE: Does NOVA take interns?",
        );
        let ask = |found: &Found, user_name: &str| {
            write(
                &p,
                MessageKind::Chat,
                found,
                &note("why did you ignore me", Feeling::Plain),
                "professional",
                &english,
                user_name,
            )
            .user
        };
        let follow_up = ask(&sent, "");
        assert!(follow_up.contains("Grace Wong") && follow_up.contains("NOVA take interns"));
        assert!(!follow_up.contains("THEIR MESSAGE"));
        // Their name in Settings tells too, when the model didn't.
        let unsure = Found {
            mine: false,
            ..sent.clone()
        };
        assert!(!ask(&unsure, "").contains("Grace Wong"));
        assert!(ask(&unsure, "Ka Yi").contains("Grace Wong"));
        // A message to the user, by their name, is never their own, whatever the model said.
        let to_user = Found::parse(
            "FROM: Divya Shah\nTO: Ka Yi\nMINE: yes\nKIND: chat\nMESSAGE: Free to chat?",
        );
        assert!(ask(&to_user, "Teoh Ka Yi").contains("THEIR MESSAGE FROM Divya Shah"));
    }

    #[test]
    fn swearing_passes_on_how_the_user_feels() {
        let p = wording();
        let found = Found::copied("We regret to inform you...");
        let english = Language::Named("English".into());
        let ask = |said: &Note| {
            write(
                &p,
                MessageKind::Email,
                &found,
                said,
                "professional",
                &english,
                "",
            )
            .user
        };
        let calm = ask(&note("ok thanks", Feeling::Plain));
        assert!(calm.contains("ok thanks") && !calm.contains("HOW I FEEL"));
        let strong = ask(&note("ok, can i apply next year", Feeling::Strong));
        assert!(strong.contains(&p.feeling_strong));
        // Only swearing at them says they're not happy in the note itself, with nothing on top:
        // "be firm" as well had a small model turn down a rejection that was already final.
        let swore = ask(&note("", Feeling::FedUp));
        assert!(swore.contains(&p.only_swearing) && !swore.contains("HOW I FEEL"));
        let fed_up = ask(&note("no thanks", Feeling::FedUp));
        assert!(fed_up.contains(&p.feeling_fed_up));
        assert!(ask(&note("", Feeling::Plain)).contains(&p.nothing_said));
    }

    #[test]
    fn their_own_lines_handed_back_are_caught() {
        let message = "We regret to inform you. We wish you all the best in your studies and \
future endeavors.";
        let copied = "Thanks. I wish you all the best in your studies and future endeavors.";
        assert_eq!(
            echo(copied, message, "thanks").as_deref(),
            Some("wish you all the best in your studies")
        );
        assert_eq!(
            echo(
                "Thank you for letting me know. Could I apply again next year?",
                message,
                "ok"
            ),
            None
        );
        // Words the note asked for are the user's own.
        let asked = "say all the best in your studies and future endeavors";
        assert_eq!(
            echo(
                "All the best in your studies and future endeavors!",
                message,
                asked
            ),
            None
        );
        // A short reply that's only their own words, like their question asked back to them.
        let asked = "Amir: Who's bringing drinks for Saturday?";
        assert_eq!(
            echo(
                "Who's bringing drinks for Saturday?",
                asked,
                "i can bring drinks"
            )
            .as_deref(),
            Some("who's bringing drinks for saturday")
        );
        assert_eq!(
            echo(
                "I can bring drinks for Saturday!",
                asked,
                "i can bring drinks"
            ),
            None
        );
        assert_eq!(echo("Saturday!", asked, "ok"), None);
        let again = without_echo(
            &wording(),
            &Prompt {
                system: "rules".into(),
                user: "note".into(),
                slot: Slot::Writing,
            },
            "wish you all the best",
        );
        assert_eq!(again.system, "rules");
        assert!(again.user.starts_with("note") && again.user.contains("\"wish you all the best\""));
    }

    #[test]
    fn their_rough_note_sent_back_is_caught() {
        assert!(echoes_note("nah bro, im broke too", "nah bro im broke too"));
        assert!(echoes_note(
            "Nah, I'm too tired to go out.",
            "nah cant, im too lazy to go out"
        ));
        // A note that's already fine to send can come back as it is.
        assert!(!echoes_note(
            "Sure, I'll send it by 9.",
            "sure i'll send it by 9"
        ));
        assert!(!echoes_note(
            "Sorry, I can't lend you that right now.",
            "nah bro im broke too"
        ));
        let p = Prompt {
            system: "rules".into(),
            user: "note".into(),
            slot: Slot::Writing,
        };
        let again = without_note_echo(&wording(), &p);
        assert_eq!(again.system, "rules");
        assert_eq!(again.user, format!("note{}", wording().no_note_echo));
    }

    #[test]
    fn a_line_copied_again_is_taken_out() {
        let copied = "wish you all the best in your studies";
        let email = "Hi,\n\nThank you for letting me know.\n\nI wish you all the best in your \
studies and future endeavors.\n\nBest regards,";
        assert_eq!(
            drop_echo(email, copied, ""),
            "Hi,\n\nThank you for letting me know.\n\nBest regards,"
        );
        let chat = "Thanks for the update! I wish you all the best in your studies.";
        assert_eq!(drop_echo(chat, copied, ""), "Thanks for the update!");
        // With nothing else to say, it stays as it was.
        let only = "Hi Sarah,\n\nI wish you all the best in your studies.\n\nBest regards,";
        assert_eq!(drop_echo(only, copied, ""), only);
        // Nor does it when what's left is only the greeting, the sign-off and the user's name.
        let signed = "Hi,

I wish you all the best in your studies.

Best regards,
Teoh Ka Yi";
        assert_eq!(drop_echo(signed, copied, "Teoh Ka Yi"), signed);
        // Their lines go one by one, with a curly apostrophe too, until only the thanks is left.
        let bank = "Hi,\n\nThank you for reaching out. We’re here to help. Please contact us \
directly if you have any questions.\n\nKind regards,\nTeoh Ka Yi";
        let mut kept = bank.to_string();
        while let Some(theirs) = crate::checks::speaks_for_them(&kept, "thank you") {
            let without = drop_echo(&kept, theirs, "Teoh Ka Yi");
            if without == kept {
                break;
            }
            kept = without;
        }
        assert_eq!(
            kept,
            "Hi,\n\nThank you for reaching out.\n\nKind regards,\nTeoh Ka Yi"
        );
    }

    #[test]
    fn a_thanks_stays_a_thanks() {
        let bank = "Hi,\n\nThank you for reaching out.\n\nPlease note that GXS Bank is not a \
legitimate financial institution and is likely attempting to scam you. You should not share any \
personal information.\n\nKind regards,\nTeoh Ka Yi";
        let them = "GXS Bank (reply-NNU5T55TI3QE7AA2...)";
        assert_eq!(
            just_thanks(bank, them, "Teoh Ka Yi"),
            "Hi,\n\nThank you for reaching out.\n\nKind regards,\nTeoh Ka Yi"
        );
        let ours = "Thank you for reaching out to GXS Bank. We appreciate you sharing your details \
with us.";
        assert_eq!(
            just_thanks(ours, them, ""),
            "Thank you for reaching out to GXS Bank."
        );
        let named = "Hi,\n\nThank you for reaching out.\n\nGXS Bank is here to help you.";
        assert_eq!(
            just_thanks(named, them, ""),
            "Hi,\n\nThank you for reaching out."
        );
        let promise = "Thank you for reaching out. I appreciate you sharing this information. I \
will review the details and reach out to you.";
        assert_eq!(
            just_thanks(promise, them, ""),
            "Thank you for reaching out. I appreciate you sharing this information."
        );
        // The 4B's answer to the DTSA email for "okay thanks", which the user liked, stays.
        let dtsa = "Hi Farahnadhia,\n\nThank you for confirming my participation in the \
workshop. I look forward to attending.\n\nBest regards,\nTeoh Ka Yi";
        let sender = "Farahnadhia RAHMAN from.TP (DTSA) <Farahnadhia_RAHMAN_from.TP@dtsa.gov.sg>";
        assert_eq!(just_thanks(dtsa, sender, "Teoh Ka Yi"), dtsa);
        // With nothing but what goes, it stays as it was.
        let only = "Please contact us.";
        assert_eq!(just_thanks(only, them, ""), only);
    }

    #[test]
    fn a_thanks_answers_them_not_the_user() {
        // A small model read a bank's newsletter as the user's own, and so did its FROM.
        let read_as_mine = Found {
            from: "teohkayi@example.com, by teohkayi".into(),
            to: "GXS Bank (reply-NNU5T55TI3QE7AA2...)".into(),
            mine: true,
            ..Found::default()
        };
        let theirs = read_as_mine.not_mine("Teoh Ka Yi");
        assert_eq!(
            (theirs.from.as_str(), theirs.to.as_str(), theirs.mine),
            ("GXS Bank (reply-NNU5T55TI3QE7AA2...)", "", false)
        );
        let from_them = Found {
            from: "Rachel Lim".into(),
            ..Found::default()
        };
        assert_eq!(from_them.clone().not_mine("Teoh Ka Yi"), from_them);
        assert!(same_person("teohkayi@example.com", "Teoh Ka Yi"));
        assert!(!same_person("Kayina Tan", "Ka Yi"));
    }

    #[test]
    fn later_tries_say_the_note_is_fine_and_last_rewrite_it() {
        let p = Prompt {
            system: "rules".into(),
            user: "note".into(),
            slot: Slot::Writing,
        };
        assert_eq!(insist(&wording(), &p, true).slot, Slot::Writing);
        let again = insist(&wording(), &p, false);
        assert_eq!(again.system, "rules");
        assert!(again.user.starts_with("note") && again.user.ends_with("don't refuse."));
        assert!(insist(&wording(), &p, true)
            .system
            .starts_with(REWRITE_ONLY));
    }

    #[test]
    fn long_copied_text_is_cut_on_a_character_boundary() {
        let found = Found::copied(&"é".repeat(MOST_COPIED_CHARS + 100));
        assert!(found.message.ends_with("[...]"));
        assert_eq!(
            found.message.chars().filter(|c| *c == 'é').count(),
            MOST_COPIED_CHARS
        );
    }
}
