//! Takes out what a stammer leaves in a transcript: a word said twice ("I I think"), a phrase said
//! again before the sentence carries on ("I want to I want to go"), a word cut off and said again
//! ("th- the"), and ums and uhs. Words people say twice on purpose, like "had had" or "very very",
//! stay.

use crate::numbers::{number_word, tokens, Token};

/// Said twice on purpose: "I had had enough", "a very very long day", "no, no".
const MEANT_TWICE: [&str; 29] = [
    "had", "that", "no", "bye", "very", "really", "much", "many", "far", "long", "way", "super",
    "ha", "knock", "yeah", "night", "hear", "blah", "chop", "tut", "la", "go", "please", "yes",
    "okay", "ok", "wait", "hey", "hi",
];
const FILLERS: [&str; 10] = [
    "um", "uh", "uhm", "umm", "uhh", "erm", "er", "ah", "hmm", "mm",
];
/// What can come between two copies of a stammer: nothing, a comma, or a dash or dots as speech
/// recognition writes a break.
const PAUSES: [&str; 8] = [
    "", ",", "-", "--", "\u{2014}", "\u{2013}", "...", "\u{2026}",
];
const DASHES: [&str; 3] = ["-", "\u{2014}", "\u{2013}"];
const SENTENCE_ENDS: [char; 3] = ['.', '!', '?'];
/// The longest phrase looked for said twice.
const MOST_PHRASE: usize = 4;

fn number_or_digits(word: &str) -> bool {
    (!word.is_empty() && word.chars().all(|c| c.is_ascii_digit())) || number_word(word)
}

fn paused(token: &Token) -> bool {
    PAUSES.contains(&token.trail.trim())
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn starts_upper(word: &str) -> bool {
    word.starts_with(char::is_uppercase)
}

fn drop_fillers(mut tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    for k in 0..tokens.len() {
        let filler = tokens[k].clone();
        if !FILLERS.contains(&filler.word().as_str()) {
            out.push(filler);
            continue;
        }
        // "Um, so we..." starts a sentence: the next word takes its capital.
        let starts = out
            .last()
            .is_none_or(|last| last.trail.trim_end().ends_with(SENTENCE_ENDS));
        if starts {
            if let Some(next) = tokens.get_mut(k + 1) {
                next.lead.insert_str(0, &filler.lead);
                if next.core.starts_with(char::is_lowercase) {
                    next.core = capitalized(&next.core);
                }
            }
        }
        // "...and um." keeps its full stop.
        let ending = filler.trail.trim();
        if ending.ends_with(SENTENCE_ENDS) {
            if let Some(last) = out.last_mut().filter(|last| last.trail.is_empty()) {
                last.trail = ending.to_string();
                last.space = filler.space;
            }
        }
    }
    out
}

/// "th- the" and "I- I": a piece of a word broken off with a dash, then the whole word. And
/// "w-w-what" written as one.
fn drop_cut_off(mut tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::new();
    for k in 0..tokens.len() {
        let piece = tokens[k].clone();
        if let Some(next) = tokens.get_mut(k + 1) {
            let broken_off = DASHES.contains(&piece.trail.as_str())
                && !piece.core.is_empty()
                && next.lead.is_empty()
                && next.word().starts_with(&piece.word());
            if broken_off {
                next.lead = piece.lead;
                if starts_upper(&piece.core) {
                    next.core = capitalized(&next.core);
                }
                continue;
            }
        }
        let parts: Vec<&str> = piece.core.split('-').collect();
        let Some((whole, pieces)) = parts.split_last() else {
            out.push(piece);
            continue;
        };
        // One letter, or a piece said more than once: "re-read" is a word, "w-what" and
        // "wh-wh-what" aren't.
        let stammered = pieces.iter().all(|p| p.chars().count() == 1) || pieces.len() > 1;
        let starts_it = pieces.iter().all(|p| {
            !p.is_empty()
                && whole.to_lowercase().starts_with(&p.to_lowercase())
                && p.chars().count() < whole.chars().count()
        });
        if !pieces.is_empty() && stammered && starts_it {
            let core = if starts_upper(parts[0]) {
                capitalized(whole)
            } else {
                whole.to_string()
            };
            out.push(Token { core, ..piece });
            continue;
        }
        out.push(piece);
    }
    out
}

fn drop_repeats(mut out: Vec<Token>) -> Vec<Token> {
    let mut i = 0;
    while i < out.len() {
        let mut dropped = false;
        for size in (1..=MOST_PHRASE).rev() {
            let after = i + 2 * size;
            if after > out.len() {
                continue;
            }
            let (first, second) = (&out[i..i + size], &out[i + size..after]);
            let words: Vec<String> = first.iter().map(Token::word).collect();
            if words != second.iter().map(Token::word).collect::<Vec<_>>()
                || words.iter().any(|w| w.is_empty() || number_or_digits(w))
            {
                continue;
            }
            // Nothing but pauses inside the copies, or between them.
            let inner = first[..size - 1].iter().all(|t| t.trail.is_empty())
                && second[..size - 1].iter().all(|t| t.trail.is_empty());
            if !inner || !paused(&first[size - 1]) || !second[0].lead.is_empty() {
                continue;
            }
            if size == 1 && MEANT_TWICE.contains(&words[0].as_str()) {
                continue;
            }
            // A phrase said again for emphasis ends there: "thank you, thank you." Said again as
            // a stammer, the sentence carries on: "I want to I want to go".
            let follows =
                after < out.len() && paused(&second[size - 1]) && out[after].lead.is_empty();
            if size > 1 && !follows {
                continue;
            }
            // The first copy goes, keeping how it began: "The the dog" is "The dog".
            let (head, keep) = (&first[0], &second[0]);
            let core = if starts_upper(&head.core) && !starts_upper(&keep.core) {
                capitalized(&keep.core)
            } else {
                keep.core.clone()
            };
            let merged = Token {
                lead: format!("{}{}", head.lead, keep.lead),
                core,
                trail: keep.trail.clone(),
                space: keep.space.clone(),
            };
            out[i + size] = merged;
            out.drain(i..i + size);
            dropped = true;
            break;
        }
        if !dropped {
            i += 1;
        }
    }
    out
}

/// The transcript with stammering taken out. With nothing left but an "um", it stays as it was.
pub fn smooth(text: &str) -> String {
    let tokens = drop_repeats(drop_cut_off(drop_fillers(tokens(text))));
    let out: String = tokens
        .iter()
        .map(|t| format!("{}{}{}{}", t.lead, t.core, t.trail, t.space))
        .collect();
    if out.trim().is_empty() {
        text.to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::smooth;

    /// What dictation types for each thing heard, before the writing model sees it.
    const HEARD_AND_TYPED: [(&str, &str); 39] = [
        // Words said twice or more.
        ("I I I think we should go.", "I think we should go."),
        ("Send me the the report.", "Send me the report."),
        ("Send me the, the report.", "Send me the report."),
        ("The the meeting moved.", "The meeting moved."),
        (
            "We we need it, it's it's urgent.",
            "We need it, it's urgent.",
        ),
        ("My my son is sick.", "My son is sick."),
        // Said twice on purpose.
        ("I had had enough.", "I had had enough."),
        ("I think that that is right.", "I think that that is right."),
        (
            "It was a very very long day.",
            "It was a very very long day.",
        ),
        ("No, no, that's not it.", "No, no, that's not it."),
        ("Bye bye.", "Bye bye."),
        ("Thank you, thank you.", "Thank you, thank you."),
        // Phrases said again.
        ("I want to I want to go home.", "I want to go home."),
        ("Can you, can you send it?", "Can you send it?"),
        (
            "We need to we need to finish the slides.",
            "We need to finish the slides.",
        ),
        ("It's in the in the drawer.", "It's in the drawer."),
        ("Thank you, thank you so much.", "Thank you so much."),
        ("Let me, let me know.", "Let me know."),
        // Cut off.
        ("Th- the price is high.", "The price is high."),
        ("I- I don't know.", "I don't know."),
        ("W-w-what happened?", "What happened?"),
        ("Wh-wh-why?", "Why?"),
        ("Please re-read it.", "Please re-read it."),
        ("Pre- and post-war.", "Pre- and post-war."),
        ("So-so.", "So-so."),
        // Fillers.
        ("Um, so we should go.", "So we should go."),
        ("I think, um, we should go.", "I think, we should go."),
        ("We should uh go now.", "We should go now."),
        ("Um.", "Um."),
        ("Hmm, let me think.", "Let me think."),
        // Numbers are left alone.
        ("The code is 444.", "The code is 444."),
        ("Two, two of them.", "Two, two of them."),
        // Nothing to do.
        ("Hello there.", "Hello there."),
        ("", ""),
        ("Go. Go.", "Go. Go."),
        // Stutters on words that are sometimes said twice on purpose.
        ("The project is is delayed.", "The project is delayed."),
        ("So so I was thinking.", "So I was thinking."),
        ("Do do you know?", "Do you know?"),
        ("It's too too late.", "It's too late."),
    ];

    #[test]
    fn stammering_comes_out() {
        for (heard, typed) in HEARD_AND_TYPED {
            assert_eq!(smooth(heard), typed, "heard {heard:?}");
        }
    }
}
