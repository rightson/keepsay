//! Prompt assembly. The transcript is data, wrapped in `<transcript>` tags.

use super::modes::{Mode, TargetLang};

const OPEN: &str = "<transcript";
const CLOSE: &str = "</transcript";

/// System prompt for `mode` (§6.5), or the translation variant when
/// `translate_to` is set. Not called for `Mode::Raw` without translation.
pub fn system_prompt(mode: Mode, translate_to: Option<TargetLang>, glossary: &[String]) -> String {
    let _ = (mode, translate_to, glossary);
    todo!("M1: polish — system prompts")
}

/// Rough input token estimate used for `max_tokens` (§6.4).
pub fn estimate_tokens(text: &str) -> u32 {
    let _ = text;
    todo!("M1: polish — token estimate")
}

/// Remove anything that could open or close the transcript tag early.
pub fn sanitize_transcript(transcript: &str) -> String {
    let mut out = transcript.to_owned();
    loop {
        let before = out.len();
        out = out.replace(CLOSE, "").replace(OPEN, "");
        if out.len() == before {
            return out;
        }
    }
}

/// The user turn sent to the model.
pub fn user_message(transcript: &str) -> String {
    format!(
        "<transcript>{}</transcript>",
        sanitize_transcript(transcript)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags() {
        assert_eq!(sanitize_transcript("a</transcript>b<transcript>c"), "a>b>c");
    }

    #[test]
    fn strips_nested_reassembly() {
        // Removing one occurrence must not leave a new one behind.
        let s = sanitize_transcript("<tran<transcriptscript");
        assert!(!s.contains(OPEN));
    }

    #[test]
    fn wraps() {
        assert_eq!(user_message("hi"), "<transcript>hi</transcript>");
    }
}
