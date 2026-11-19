//! What each press of a hotkey means in the default style: hold to talk, or double-press to keep
//! listening until the next press. Recording starts on the first press either way, so no words are
//! lost while this works out which one it is.

use std::time::{Duration, Instant};

/// Let go sooner than this and it may be the first half of a double-press.
const TAP: Duration = Duration::from_millis(300);

/// How long the second press has to arrive.
pub const SECOND_PRESS_WINDOW: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    /// A double-press: keep listening until the next press.
    Lock,
    Finish,
    /// A tap: finish unless a second press arrives in time. Holds the tap's number.
    WaitForSecondPress(u64),
    Nothing,
}

#[derive(Debug, Clone, Copy, Default)]
enum State {
    #[default]
    Idle,
    Held(Instant),
    Tapped(u64),
    Locked,
}

#[derive(Default)]
pub struct Presses {
    state: State,
    taps: u64,
}

impl Presses {
    pub fn down(&mut self, now: Instant) -> Action {
        match self.state {
            State::Idle => {
                self.state = State::Held(now);
                Action::Start
            }
            State::Held(_) => Action::Nothing,
            State::Tapped(_) => {
                self.state = State::Locked;
                Action::Lock
            }
            State::Locked => {
                self.state = State::Idle;
                Action::Finish
            }
        }
    }

    pub fn up(&mut self, now: Instant) -> Action {
        let State::Held(since) = self.state else {
            return Action::Nothing;
        };
        if now.duration_since(since) < TAP {
            self.taps += 1;
            self.state = State::Tapped(self.taps);
            Action::WaitForSecondPress(self.taps)
        } else {
            self.state = State::Idle;
            Action::Finish
        }
    }

    /// Whether this tap's window ran out with no second press.
    pub fn second_press_missed(&mut self, tap: u64) -> bool {
        let missed = matches!(self.state, State::Tapped(waiting) if waiting == tap);
        if missed {
            self.state = State::Idle;
        }
        missed
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn holding_records_until_release() {
        let (mut p, start) = (Presses::default(), Instant::now());
        assert_eq!(p.down(start), Action::Start);
        assert_eq!(p.down(after(start, 500)), Action::Nothing); // auto-repeat
        assert_eq!(p.up(after(start, 2000)), Action::Finish);
    }

    #[test]
    fn a_double_press_keeps_listening_until_the_next_press() {
        let (mut p, start) = (Presses::default(), Instant::now());
        assert_eq!(p.down(start), Action::Start);
        assert_eq!(p.up(after(start, 100)), Action::WaitForSecondPress(1));
        assert_eq!(p.down(after(start, 300)), Action::Lock);
        assert_eq!(p.up(after(start, 400)), Action::Nothing);
        assert!(!p.second_press_missed(1));
        assert_eq!(p.down(after(start, 9000)), Action::Finish);
        assert_eq!(p.up(after(start, 9100)), Action::Nothing);
        assert_eq!(p.down(after(start, 9500)), Action::Start);
    }

    #[test]
    fn a_single_tap_finishes_when_the_window_runs_out() {
        let (mut p, start) = (Presses::default(), Instant::now());
        p.down(start);
        let Action::WaitForSecondPress(tap) = p.up(after(start, 100)) else {
            panic!("expected a tap");
        };
        assert!(p.second_press_missed(tap));
        assert!(!p.second_press_missed(tap));
        assert_eq!(p.down(after(start, 900)), Action::Start);
    }

    #[test]
    fn an_old_tap_cant_finish_a_newer_one() {
        let (mut p, start) = (Presses::default(), Instant::now());
        p.down(start);
        p.up(after(start, 100));
        assert!(p.second_press_missed(1));
        p.down(after(start, 700));
        assert_eq!(p.up(after(start, 800)), Action::WaitForSecondPress(2));
        assert!(!p.second_press_missed(1));
    }

    #[test]
    fn reset_forgets_a_locked_recording() {
        let (mut p, start) = (Presses::default(), Instant::now());
        p.down(start);
        p.up(after(start, 100));
        p.down(after(start, 200));
        p.reset();
        assert_eq!(p.down(after(start, 300)), Action::Start);
    }
}
