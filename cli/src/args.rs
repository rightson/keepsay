//! Hand-written argument parsing (clap is not on the §14 whitelist).
//!
//! Error messages never echo an argument back: a user who mistakenly passes
//! their key on the command line must not see it reprinted to the terminal
//! or a CI log. Errors name the flag or position instead.

use std::fmt;
use std::path::PathBuf;

use keepsay_core::{Mode, TargetLang};

pub const USAGE: &str = "\
usage: keepsay-cli [global options] <command>

commands:
  transcribe <file.wav> --model <ggml.bin> [--lang <bcp47>]
      Local speech-to-text. Prints the transcript to stdout.
  polish (<file.wav> --model <ggml.bin> | --text-stdin)
         [--mode raw|light|polish|formal] [--translate en|zh-TW|ja] [--lang <bcp47>]
      Transcribe (or read text from stdin), then polish / polish and translate.
      Prints the result to stdout. `keepsay-cli <file.wav> ...` is shorthand.
  key set       Read the key from a hidden prompt (or stdin), verify, store.
  key status    Show whether a key is stored, and its fingerprint.
  key delete    Delete the stored key.
  key verify    Check the stored key against the API.
  models list | download <id> | import <id> <file>
  budget        Show this month's usage and limits.

global options:
  --data-dir <dir>   Settings, budget ledger and models (default: platform app data).
  --dev-mock-vault   Debug builds only: keep the key in memory for this run
                     instead of the OS secure storage. Commands that need a
                     key read it first (hidden prompt, or the first stdin line).
  -v, --verbose      Log events and timings (never content) to stderr.
  -h, --help | --version

The key is never accepted as an argument or environment variable.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Wav(PathBuf),
    TextStdin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Transcribe {
        wav: PathBuf,
        model: PathBuf,
        lang: Option<String>,
    },
    Polish {
        input: Input,
        model: Option<PathBuf>,
        lang: Option<String>,
        mode: Option<Mode>,
        translate: Option<TargetLang>,
    },
    KeySet,
    KeyStatus,
    KeyDelete,
    KeyVerify,
    ModelsList,
    ModelsDownload {
        id: String,
    },
    ModelsImport {
        id: String,
        file: PathBuf,
    },
    Budget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub data_dir: Option<PathBuf>,
    pub dev_mock_vault: bool,
    pub verbose: bool,
    pub command: Command,
}

/// Parse failures. Never carries argument text (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgError {
    MissingCommand,
    UnknownCommand,
    /// 1-based position of an argument that was not expected.
    Unexpected(usize),
    MissingValue(&'static str),
    InvalidValue(&'static str),
    Duplicate(&'static str),
    MissingInput,
    MissingModel,
    ConflictingInput,
    /// Something was passed after `key set`: maybe the key itself.
    KeyOnCommandLine,
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgError::MissingCommand => f.write_str("missing command"),
            ArgError::UnknownCommand => f.write_str("unknown command"),
            ArgError::Unexpected(pos) => write!(f, "unexpected argument at position {pos}"),
            ArgError::MissingValue(flag) => write!(f, "{flag} needs a value"),
            ArgError::InvalidValue(flag) => write!(f, "invalid value for {flag}"),
            ArgError::Duplicate(flag) => write!(f, "{flag} given more than once"),
            ArgError::MissingInput => f.write_str("give a .wav file or --text-stdin"),
            ArgError::MissingModel => f.write_str("a .wav input needs --model <ggml.bin>"),
            ArgError::ConflictingInput => f.write_str("give either a .wav file or --text-stdin"),
            ArgError::KeyOnCommandLine => f.write_str(
                "`key set` takes no arguments: the key is never accepted on the command line. \
                 Run `keepsay-cli key set` and type it at the hidden prompt, or pipe it on stdin. \
                 If you just typed a real key here, revoke it in the Anthropic Console.",
            ),
        }
    }
}

/// Positional arguments and per-command flags left after globals are taken.
#[derive(Default)]
struct Rest {
    /// (1-based position in argv, value)
    positional: Vec<(usize, String)>,
    model: Option<PathBuf>,
    lang: Option<String>,
    mode: Option<Mode>,
    translate: Option<TargetLang>,
    text_stdin: bool,
    /// (position, flag) of command flags, to reject them where they do not apply.
    flag_positions: Vec<(usize, &'static str)>,
}

fn parse_mode(s: &str) -> Option<Mode> {
    match s {
        "raw" => Some(Mode::Raw),
        "light" => Some(Mode::Light),
        "polish" => Some(Mode::Polish),
        "formal" => Some(Mode::Formal),
        _ => None,
    }
}

fn parse_lang(s: &str) -> Option<TargetLang> {
    [TargetLang::En, TargetLang::ZhTw, TargetLang::Ja]
        .into_iter()
        .find(|l| l.code().eq_ignore_ascii_case(s))
}

fn set_once<T>(slot: &mut Option<T>, v: T, flag: &'static str) -> Result<(), ArgError> {
    if slot.is_some() {
        return Err(ArgError::Duplicate(flag));
    }
    *slot = Some(v);
    Ok(())
}

/// Parse `args` (without the program name).
pub fn parse<I, S>(args: I) -> Result<Cli, ArgError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut data_dir = None;
    let mut dev_mock_vault = false;
    let mut verbose = false;
    let mut help = false;
    let mut version = false;
    let mut rest = Rest::default();

    let mut i = 0;
    while i < args.len() {
        let pos = i + 1;
        let a = args[i].as_str();
        let mut value = |flag: &'static str| -> Result<String, ArgError> {
            i += 1;
            args.get(i).cloned().ok_or(ArgError::MissingValue(flag))
        };
        match a {
            "--data-dir" => set_once(
                &mut data_dir,
                PathBuf::from(value("--data-dir")?),
                "--data-dir",
            )?,
            "--dev-mock-vault" => dev_mock_vault = true,
            "-v" | "--verbose" => verbose = true,
            "-h" | "--help" => help = true,
            "--version" => version = true,
            "--model" => {
                rest.flag_positions.push((pos, "--model"));
                set_once(&mut rest.model, PathBuf::from(value("--model")?), "--model")?;
            }
            "--lang" => {
                rest.flag_positions.push((pos, "--lang"));
                set_once(&mut rest.lang, value("--lang")?, "--lang")?;
            }
            "--mode" => {
                rest.flag_positions.push((pos, "--mode"));
                let m = parse_mode(&value("--mode")?).ok_or(ArgError::InvalidValue("--mode"))?;
                set_once(&mut rest.mode, m, "--mode")?;
            }
            "--translate" => {
                rest.flag_positions.push((pos, "--translate"));
                let l = parse_lang(&value("--translate")?)
                    .ok_or(ArgError::InvalidValue("--translate"))?;
                set_once(&mut rest.translate, l, "--translate")?;
            }
            "--text-stdin" => {
                rest.flag_positions.push((pos, "--text-stdin"));
                if rest.text_stdin {
                    return Err(ArgError::Duplicate("--text-stdin"));
                }
                rest.text_stdin = true;
            }
            "--" => {
                for (j, v) in args.iter().enumerate().skip(i + 1) {
                    rest.positional.push((j + 1, v.clone()));
                }
                break;
            }
            _ if a.starts_with('-') && a.len() > 1 => return Err(ArgError::Unexpected(pos)),
            _ => rest.positional.push((pos, a.to_owned())),
        }
        i += 1;
    }

    let command = if help {
        Command::Help
    } else if version {
        Command::Version
    } else {
        command(rest)?
    };
    Ok(Cli {
        data_dir,
        dev_mock_vault,
        verbose,
        command,
    })
}

fn no_flags(rest: &Rest) -> Result<(), ArgError> {
    match rest.flag_positions.first() {
        Some(&(p, _)) => Err(ArgError::Unexpected(p)),
        None => Ok(()),
    }
}

fn exactly<const N: usize>(pos: &[(usize, String)]) -> Result<[String; N], ArgError> {
    if let Some((p, _)) = pos.get(N) {
        return Err(ArgError::Unexpected(*p));
    }
    let v: Vec<String> = pos.iter().map(|(_, s)| s.clone()).collect();
    v.try_into().map_err(|_| ArgError::MissingValue("command"))
}

fn command(rest: Rest) -> Result<Command, ArgError> {
    let Some((_, first)) = rest.positional.first().cloned() else {
        return Err(ArgError::MissingCommand);
    };
    let args = &rest.positional[1..];
    match first.as_str() {
        "transcribe" => {
            if let Some(&(p, _)) = rest
                .flag_positions
                .iter()
                .find(|(_, f)| matches!(*f, "--mode" | "--translate" | "--text-stdin"))
            {
                return Err(ArgError::Unexpected(p));
            }
            let [wav] = exactly::<1>(args).map_err(|e| match e {
                ArgError::MissingValue(_) => ArgError::MissingInput,
                e => e,
            })?;
            Ok(Command::Transcribe {
                wav: PathBuf::from(wav),
                model: rest.model.ok_or(ArgError::MissingModel)?,
                lang: rest.lang,
            })
        }
        "polish" => polish(args, rest.clone_flags()),
        "key" => {
            let sub = args.first().map(|(_, s)| s.as_str());
            if sub == Some("set") && args.len() > 1 {
                return Err(ArgError::KeyOnCommandLine);
            }
            no_flags(&rest)?;
            if let Some((p, _)) = args.get(1) {
                return Err(ArgError::Unexpected(*p));
            }
            match sub {
                Some("set") => Ok(Command::KeySet),
                Some("status") => Ok(Command::KeyStatus),
                Some("delete") => Ok(Command::KeyDelete),
                Some("verify") => Ok(Command::KeyVerify),
                _ => Err(ArgError::UnknownCommand),
            }
        }
        "models" => {
            no_flags(&rest)?;
            match args.first().map(|(_, s)| s.as_str()) {
                Some("list") => {
                    exactly::<0>(&args[1..])?;
                    Ok(Command::ModelsList)
                }
                Some("download") => {
                    let [id] = exactly::<1>(&args[1..])?;
                    Ok(Command::ModelsDownload { id })
                }
                Some("import") => {
                    let [id, file] = exactly::<2>(&args[1..])?;
                    Ok(Command::ModelsImport {
                        id,
                        file: PathBuf::from(file),
                    })
                }
                _ => Err(ArgError::UnknownCommand),
            }
        }
        "budget" => {
            no_flags(&rest)?;
            exactly::<0>(args)?;
            Ok(Command::Budget)
        }
        // Shorthand from US-M1-23: `keepsay-cli <file.wav> [--mode] [--translate]`.
        f if f.to_ascii_lowercase().ends_with(".wav") => {
            polish(&rest.positional, rest.clone_flags())
        }
        _ => Err(ArgError::UnknownCommand),
    }
}

/// Flags only (positional list emptied), so `polish` can take its own slice.
struct Flags {
    model: Option<PathBuf>,
    lang: Option<String>,
    mode: Option<Mode>,
    translate: Option<TargetLang>,
    text_stdin: bool,
}

impl Rest {
    fn clone_flags(&self) -> Flags {
        Flags {
            model: self.model.clone(),
            lang: self.lang.clone(),
            mode: self.mode,
            translate: self.translate,
            text_stdin: self.text_stdin,
        }
    }
}

fn polish(args: &[(usize, String)], f: Flags) -> Result<Command, ArgError> {
    if let Some((p, _)) = args.get(1) {
        return Err(ArgError::Unexpected(*p));
    }
    let input = match (args.first(), f.text_stdin) {
        (Some(_), true) => return Err(ArgError::ConflictingInput),
        (Some((_, wav)), false) => Input::Wav(PathBuf::from(wav)),
        (None, true) => Input::TextStdin,
        (None, false) => return Err(ArgError::MissingInput),
    };
    if matches!(input, Input::Wav(_)) && f.model.is_none() {
        return Err(ArgError::MissingModel);
    }
    Ok(Command::Polish {
        input,
        model: f.model,
        lang: f.lang,
        mode: f.mode,
        translate: f.translate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Cli, ArgError> {
        parse(s.split_whitespace())
    }

    fn cmd(s: &str) -> Command {
        p(s).unwrap().command
    }

    #[test]
    fn help_and_version() {
        assert_eq!(cmd("--help"), Command::Help);
        assert_eq!(cmd("-h key set"), Command::Help);
        assert_eq!(cmd("--version"), Command::Version);
        assert_eq!(p(""), Err(ArgError::MissingCommand));
        assert_eq!(p("frobnicate"), Err(ArgError::UnknownCommand));
    }

    #[test]
    fn transcribe() {
        assert_eq!(
            cmd("transcribe a.wav --model m.bin"),
            Command::Transcribe {
                wav: "a.wav".into(),
                model: "m.bin".into(),
                lang: None
            }
        );
        assert_eq!(
            cmd("transcribe --lang en --model m.bin a.wav"),
            Command::Transcribe {
                wav: "a.wav".into(),
                model: "m.bin".into(),
                lang: Some("en".into())
            }
        );
        assert_eq!(p("transcribe a.wav"), Err(ArgError::MissingModel));
        assert_eq!(p("transcribe --model m.bin"), Err(ArgError::MissingInput));
        assert_eq!(
            p("transcribe a.wav b.wav --model m"),
            Err(ArgError::Unexpected(3))
        );
        assert_eq!(
            p("transcribe a.wav --model m --mode raw"),
            Err(ArgError::Unexpected(5))
        );
        assert_eq!(
            p("transcribe a.wav --model"),
            Err(ArgError::MissingValue("--model"))
        );
    }

    #[test]
    fn polish_variants() {
        assert_eq!(
            cmd("polish a.wav --model m --mode formal --translate zh-TW"),
            Command::Polish {
                input: Input::Wav("a.wav".into()),
                model: Some("m".into()),
                lang: None,
                mode: Some(Mode::Formal),
                translate: Some(TargetLang::ZhTw),
            }
        );
        assert_eq!(
            cmd("polish --text-stdin --translate JA"),
            Command::Polish {
                input: Input::TextStdin,
                model: None,
                lang: None,
                mode: None,
                translate: Some(TargetLang::Ja),
            }
        );
        assert_eq!(p("polish"), Err(ArgError::MissingInput));
        assert_eq!(p("polish a.wav"), Err(ArgError::MissingModel));
        assert_eq!(
            p("polish a.wav --text-stdin --model m"),
            Err(ArgError::ConflictingInput)
        );
        assert_eq!(
            p("polish --text-stdin --mode loud"),
            Err(ArgError::InvalidValue("--mode"))
        );
        assert_eq!(
            p("polish --text-stdin --translate fr"),
            Err(ArgError::InvalidValue("--translate"))
        );
        assert_eq!(
            p("polish --text-stdin --mode raw --mode light"),
            Err(ArgError::Duplicate("--mode"))
        );
    }

    #[test]
    fn bare_wav_is_polish_shorthand() {
        assert_eq!(
            cmd("Talk.WAV --model m --translate en"),
            Command::Polish {
                input: Input::Wav("Talk.WAV".into()),
                model: Some("m".into()),
                lang: None,
                mode: None,
                translate: Some(TargetLang::En),
            }
        );
    }

    #[test]
    fn globals_anywhere() {
        let c = p("key --dev-mock-vault status --data-dir /tmp/x -v").unwrap();
        assert!(c.dev_mock_vault && c.verbose);
        assert_eq!(c.data_dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(c.command, Command::KeyStatus);
    }

    #[test]
    fn key_commands() {
        assert_eq!(cmd("key set"), Command::KeySet);
        assert_eq!(cmd("key status"), Command::KeyStatus);
        assert_eq!(cmd("key delete"), Command::KeyDelete);
        assert_eq!(cmd("key verify"), Command::KeyVerify);
        assert_eq!(p("key"), Err(ArgError::UnknownCommand));
        assert_eq!(p("key status extra"), Err(ArgError::Unexpected(3)));
        assert_eq!(p("key status --mode raw"), Err(ArgError::Unexpected(3)));
    }

    #[test]
    fn key_on_command_line_is_refused_without_echo() {
        let fake = "sk-ant-test-CANARY-7f3a";
        let e = parse(["key", "set", fake]).unwrap_err();
        assert_eq!(e, ArgError::KeyOnCommandLine);
        assert!(!e.to_string().contains("CANARY"));
        // Any other position: the error names the position, not the text.
        for args in [vec![fake], vec!["polish", "--text-stdin", fake]] {
            let e = parse(args).unwrap_err();
            assert!(!e.to_string().contains("CANARY"));
            assert!(!format!("{e:?}").contains("CANARY"));
        }
        let e = parse(["--model", "m", "--sk-ant-CANARY"]).unwrap_err();
        assert!(!e.to_string().contains("CANARY"));
    }

    #[test]
    fn models_and_budget() {
        assert_eq!(cmd("models list"), Command::ModelsList);
        assert_eq!(
            cmd("models download whisper-small"),
            Command::ModelsDownload {
                id: "whisper-small".into()
            }
        );
        assert_eq!(
            cmd("models import whisper-small ./f.bin"),
            Command::ModelsImport {
                id: "whisper-small".into(),
                file: "./f.bin".into()
            }
        );
        assert_eq!(p("models download"), Err(ArgError::MissingValue("command")));
        assert_eq!(p("models list x"), Err(ArgError::Unexpected(3)));
        assert_eq!(p("models"), Err(ArgError::UnknownCommand));
        assert_eq!(cmd("budget"), Command::Budget);
        assert_eq!(p("budget now"), Err(ArgError::Unexpected(2)));
    }

    #[test]
    fn double_dash_ends_flags() {
        assert_eq!(
            cmd("transcribe --model m -- -odd.wav"),
            Command::Transcribe {
                wav: "-odd.wav".into(),
                model: "m".into(),
                lang: None
            }
        );
    }
}
