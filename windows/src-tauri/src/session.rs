//! Toggle-mode dictation session (FR-2).
//!
//! Press either hotkey once to start recording and again to stop and
//! process. Whether the output is translated is decided by the combination
//! that *started* the recording; either combination stops it. Esc while
//! recording cancels. Presses while processing are ignored.

/// What the user asked for when recording started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The configured default mode.
    Polish,
    /// Always polish + translate (FR-8).
    PolishTranslate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Recording(Action),
    Processing(Action),
}

/// What the caller must do after an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    StartRecording(Action),
    StopAndProcess(Action),
    Cancel,
    Ignore,
}

#[derive(Debug)]
pub struct Session {
    phase: Phase,
}

impl Default for Session {
    fn default() -> Self {
        Self { phase: Phase::Idle }
    }
}

impl Session {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn on_hotkey(&mut self, pressed: Action) -> Step {
        match self.phase {
            Phase::Idle => {
                self.phase = Phase::Recording(pressed);
                Step::StartRecording(pressed)
            }
            Phase::Recording(started) => {
                self.phase = Phase::Processing(started);
                Step::StopAndProcess(started)
            }
            Phase::Processing(_) => Step::Ignore,
        }
    }

    pub fn on_escape(&mut self) -> Step {
        match self.phase {
            Phase::Recording(_) => {
                self.phase = Phase::Idle;
                Step::Cancel
            }
            _ => Step::Ignore,
        }
    }

    /// Recording could not start (no microphone, permission denied).
    pub fn on_start_failed(&mut self) {
        if matches!(self.phase, Phase::Recording(_)) {
            self.phase = Phase::Idle;
        }
    }

    /// Processing finished, successfully or not.
    pub fn on_done(&mut self) {
        if matches!(self.phase, Phase::Processing(_)) {
            self.phase = Phase::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_uses_starting_action() {
        let mut s = Session::default();
        assert_eq!(
            s.on_hotkey(Action::PolishTranslate),
            Step::StartRecording(Action::PolishTranslate)
        );
        // Stopping with the other combination keeps the translate decision.
        assert_eq!(
            s.on_hotkey(Action::Polish),
            Step::StopAndProcess(Action::PolishTranslate)
        );
        assert_eq!(s.on_hotkey(Action::Polish), Step::Ignore);
        assert_eq!(s.on_escape(), Step::Ignore);
        s.on_done();
        assert_eq!(s.phase(), Phase::Idle);
        assert_eq!(
            s.on_hotkey(Action::Polish),
            Step::StartRecording(Action::Polish)
        );
    }

    #[test]
    fn escape_cancels_only_while_recording() {
        let mut s = Session::default();
        assert_eq!(s.on_escape(), Step::Ignore);
        s.on_hotkey(Action::Polish);
        assert_eq!(s.on_escape(), Step::Cancel);
        assert_eq!(s.phase(), Phase::Idle);
    }

    #[test]
    fn start_failure_returns_to_idle() {
        let mut s = Session::default();
        s.on_hotkey(Action::Polish);
        s.on_start_failed();
        assert_eq!(s.phase(), Phase::Idle);
        // on_done does not end a recording.
        s.on_hotkey(Action::Polish);
        s.on_done();
        assert_eq!(s.phase(), Phase::Recording(Action::Polish));
    }
}
