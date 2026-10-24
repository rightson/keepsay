//! keepsay-cli: developer tool for the whole pipeline without an app
//! (PROPOSAL §13 M1, US-M1-23): wav → transcript → polish / polish and
//! translate. Run `keepsay-cli --help` for the command list.
//!
//! Output rules:
//! - stdout carries only the product output (transcript or polished text),
//!   written by [`emit_result`].
//! - Status, warnings and errors go to stderr and never contain the key,
//!   the transcript, model output or the glossary.
//! - tracing logs events, lengths and timings only.
//! - The only files written are the non-secret budget ledger after an API
//!   call and model files on `models download` / `models import`. Audio and
//!   text stay in memory (INV-5).
//! - The key comes from the OS vault, or from stdin for `key set` /
//!   `--dev-mock-vault`. Never from argv or environment variables.

mod args;
mod secret_input;
#[cfg(debug_assertions)]
mod session_vault;
mod wav;

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use keepsay_core::api::AnthropicClient;
use keepsay_core::budget::{Budget, YearMonth};
use keepsay_core::config::Config;
use keepsay_core::downloader::{self, Manifest, ModelDownloader};
use keepsay_core::vault::{self, KeyVault};
use keepsay_core::{CoreError, Mode, PolishOptions, PolishResult};

use args::{Cli, Command, Input};

/// WAV files above this size are refused before reading them.
const MAX_WAV_BYTES: u64 = 1 << 30;

#[derive(Debug)]
enum CliError {
    Core(CoreError),
    Wav(wav::WavError),
    Msg(&'static str),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Core(e) => e.fmt(f),
            CliError::Wav(e) => e.fmt(f),
            CliError::Msg(m) => f.write_str(m),
        }
    }
}

impl From<CoreError> for CliError {
    fn from(e: CoreError) -> Self {
        CliError::Core(e)
    }
}

impl From<io::Error> for CliError {
    fn from(e: io::Error) -> Self {
        CliError::Core(e.into())
    }
}

impl From<wav::WavError> for CliError {
    fn from(e: wav::WavError) -> Self {
        CliError::Wav(e)
    }
}

type Result<T, E = CliError> = std::result::Result<T, E>;

/// The single place that writes to stdout: the product output.
#[allow(clippy::print_stdout)]
fn emit_result(text: &str) {
    println!("{text}");
}

/// Panic messages can contain anything, so print the location only (§6.9).
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(loc) => eprintln!(
            "keepsay-cli: internal error at {}:{}",
            loc.file(),
            loc.line()
        ),
        None => eprintln!("keepsay-cli: internal error"),
    }));
}

fn init_tracing(verbose: bool) {
    let level = if verbose {
        tracing::Level::INFO
    } else {
        tracing::Level::WARN
    };
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_max_level(level)
        .with_target(false)
        .init();
}

fn main() -> ExitCode {
    install_panic_hook();
    // Lossy on purpose: non-UTF-8 arguments must not panic.
    let argv = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned());
    let cli = match args::parse(argv) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    init_tracing(cli.verbose);

    match cli.command {
        Command::Help => {
            eprintln!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            eprintln!("keepsay-cli {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: {}", CoreError::from(e));
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(run(&cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Help | Command::Version => Ok(()),
        Command::Transcribe { wav, model, lang } => {
            let config = load_config(&data_dir(cli)?)?;
            let text = transcribe_file(wav, model, lang.as_deref(), &config.glossary)?;
            emit_result(&text);
            Ok(())
        }
        Command::Polish {
            input,
            model,
            lang,
            mode,
            translate,
        } => {
            cmd_polish(
                cli,
                input,
                model.as_deref(),
                lang.as_deref(),
                *mode,
                *translate,
            )
            .await
        }
        Command::KeySet => cmd_key_set(cli).await,
        Command::KeyStatus => {
            let v = open_vault(cli, false)?;
            if v.exists() {
                eprintln!("key stored: {}", stored_fingerprint(v.as_ref())?);
            } else {
                eprintln!("no key stored");
            }
            Ok(())
        }
        Command::KeyDelete => {
            open_vault(cli, false)?.delete()?;
            eprintln!("key deleted");
            Ok(())
        }
        Command::KeyVerify => {
            let v = open_vault(cli, true)?;
            let config = load_config(&data_dir(cli)?)?;
            let client = AnthropicClient::new(config.proxy.as_deref())?;
            let key = v.load()?;
            let fp = vault::fingerprint(&key);
            client.verify_key(&key).await?;
            drop(key);
            eprintln!("key {fp} is valid");
            Ok(())
        }
        Command::ModelsList => {
            let m = Manifest::bundled()?;
            if m.models.is_empty() {
                eprintln!("the bundled manifest lists no models yet (US-M1-18)");
            }
            for e in &m.models {
                eprintln!(
                    "{}\t{}\t{:.0} MB\t{}",
                    e.id,
                    e.name,
                    e.size as f64 / 1_000_000.0,
                    e.license
                );
            }
            Ok(())
        }
        Command::ModelsDownload { id } => {
            let m = Manifest::bundled()?;
            let entry = m.find(id)?;
            let dir = models_dir(cli)?;
            std::fs::create_dir_all(&dir)?;
            let dl = ModelDownloader::new()?;
            let mut last = u64::MAX;
            let mut progress = |got: u64, total: u64| {
                let pct = got.saturating_mul(100) / total.max(1);
                if pct != last {
                    last = pct;
                    eprint!("\rdownloading {pct:>3}%");
                }
            };
            let path = dl.download(entry, &dir, &mut progress).await;
            eprintln!();
            eprintln!("verified and saved to {}", path?.display());
            Ok(())
        }
        Command::ModelsImport { id, file } => {
            let m = Manifest::bundled()?;
            let entry = m.find(id)?;
            let dir = models_dir(cli)?;
            std::fs::create_dir_all(&dir)?;
            let path = downloader::import(entry, file, &dir)?;
            eprintln!("verified and imported to {}", path.display());
            Ok(())
        }
        Command::Budget => {
            let dir = data_dir(cli)?;
            let config = load_config(&dir)?;
            let now = current_month();
            let budget = load_budget(&dir, now)?;
            let s = budget.status(&config.prices, &config.budget, now);
            eprintln!(
                "{:04}-{:02}: {} input / {} output tokens, about ${:.2}",
                now.year,
                now.month,
                s.month_usage.input_tokens,
                s.month_usage.output_tokens,
                s.estimated_usd
            );
            let fmt_limit = |l: Option<f64>| l.map_or("none".to_owned(), |v| format!("${v:.2}"));
            eprintln!(
                "soft limit {}, hard limit {} (prices are user-editable; verify against official pricing)",
                fmt_limit(s.soft_limit_usd),
                fmt_limit(s.hard_limit_usd)
            );
            if s.hard_limited() {
                eprintln!("hard limit reached: polish falls back to raw");
            } else if s.soft_limited() {
                eprintln!("soft limit reached");
            }
            Ok(())
        }
    }
}

async fn cmd_polish(
    cli: &Cli,
    input: &Input,
    model: Option<&Path>,
    lang: Option<&str>,
    mode: Option<Mode>,
    translate: Option<keepsay_core::TargetLang>,
) -> Result<()> {
    let dir = data_dir(cli)?;
    let config = load_config(&dir)?;
    let opts = PolishOptions {
        // FR-8: the translate action always polishes.
        mode: mode.unwrap_or(if translate.is_some() {
            Mode::Polish
        } else {
            config.mode
        }),
        translate_to: translate,
        glossary: config.glossary.clone(),
    };

    // With --dev-mock-vault the key is read before any piped text, so it
    // is always the first stdin line.
    let vault = if opts.needs_api() {
        Some(open_vault(cli, true))
    } else {
        None
    };

    let transcript = match input {
        Input::Wav(wav) => {
            let model = model.ok_or(CliError::Msg("a .wav input needs --model"))?;
            transcribe_file(wav, model, lang, &config.glossary)?
        }
        Input::TextStdin => {
            let mut s = String::new();
            io::stdin().lock().read_to_string(&mut s)?;
            s.trim().to_owned()
        }
    };

    let Some(vault) = vault else {
        emit_result(&transcript);
        return Ok(());
    };
    let vault = match vault {
        Ok(v) => v,
        Err(e) => return fall_back(&transcript, &e),
    };
    let client = match AnthropicClient::new(config.proxy.as_deref()) {
        Ok(c) => c,
        Err(e) => return fall_back(&transcript, &CliError::Core(e)),
    };

    let now = current_month();
    let mut budget = load_budget(&dir, now)?;
    let status = budget.status(&config.prices, &config.budget, now);
    if status.soft_limited() && !status.hard_limited() {
        eprintln!("warning: monthly soft limit reached");
    }

    let result: PolishResult =
        keepsay_core::polish::polish(&client, vault.as_ref(), &transcript, &opts, &status).await;
    drop(vault);

    if let Some(usage) = result.usage {
        budget.record(usage, now);
        std::fs::create_dir_all(&dir)?;
        budget.save(&budget_path(&dir))?;
    }
    if let Some(reason) = result.fallback_reason {
        eprintln!("note: output is the raw transcript ({reason:?})");
    }
    emit_result(&result.text);
    Ok(())
}

/// FR-4: any error before polishing still yields the raw transcript.
fn fall_back(transcript: &str, e: &CliError) -> Result<()> {
    eprintln!("note: output is the raw transcript ({e})");
    if matches!(e, CliError::Core(CoreError::Unsupported)) {
        eprintln!("hint: this platform has no secure storage backend; try --dev-mock-vault");
    }
    emit_result(transcript);
    Ok(())
}

async fn cmd_key_set(cli: &Cli) -> Result<()> {
    let v = open_vault(cli, false)?;
    let key = secret_input::read_key()?.ok_or(CliError::Msg("no key entered"))?;
    vault::check_format(&key)?;
    let config = load_config(&data_dir(cli)?)?;
    let client = AnthropicClient::new(config.proxy.as_deref())?;
    // FR-1: verify online first; a key that fails is not stored.
    client.verify_key(&key).await?;
    let fp = vault::fingerprint(&key);
    v.store(key)?;
    eprintln!("key {fp} verified and stored");
    if cli.dev_mock_vault {
        eprintln!("(in memory only: it is gone when this command exits)");
    }
    Ok(())
}

/// The fingerprint of the stored key. The key is dropped (and zeroized)
/// before this returns; only the fingerprint leaves.
fn stored_fingerprint(v: &dyn KeyVault) -> Result<vault::Fingerprint> {
    let key = v.load()?;
    Ok(vault::fingerprint(&key))
}

/// The OS vault, or with `--dev-mock-vault` an in-memory one. When
/// `need_key` is set, the mock is filled from stdin first.
fn open_vault(cli: &Cli, need_key: bool) -> Result<Box<dyn KeyVault>> {
    if cli.dev_mock_vault {
        #[cfg(debug_assertions)]
        {
            eprintln!("{}", session_vault::WARNING);
            if !need_key {
                return Ok(Box::new(session_vault::SessionVault::default()));
            }
            let key = secret_input::read_key()?.ok_or(CliError::Msg("no key entered"))?;
            vault::check_format(&key)?;
            return Ok(Box::new(session_vault::SessionVault::with_key(key)));
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = need_key;
            return Err(CliError::Msg(
                "--dev-mock-vault is only available in debug builds",
            ));
        }
    }
    Ok(vault::platform_vault()?)
}

/// Read and decode a WAV file into memory.
fn read_wav(wav_path: &Path) -> Result<wav::Wav> {
    if std::fs::metadata(wav_path)?.len() > MAX_WAV_BYTES {
        return Err(CliError::Msg("WAV file is larger than 1 GiB"));
    }
    let audio = wav::parse(&std::fs::read(wav_path)?)?;
    tracing::info!(
        samples = audio.samples.len(),
        sample_rate = audio.sample_rate,
        channels = audio.channels,
        "wav decoded"
    );
    Ok(audio)
}

#[cfg(feature = "whisper")]
fn transcribe_file(
    wav_path: &Path,
    model: &Path,
    lang: Option<&str>,
    glossary: &[String],
) -> Result<String> {
    use keepsay_core::SttEngine;
    use keepsay_core::stt::whisper::WhisperEngine;

    let audio = read_wav(wav_path)?;
    // The model is taken as given: this is a developer tool. App builds
    // only load files that passed the manifest SHA-256 check (INV-7).
    let engine = WhisperEngine::load(model)?;
    let opts = keepsay_core::SttOptions {
        language: lang.map(str::to_owned),
        glossary: glossary.to_vec(),
    };
    Ok(engine.transcribe(&audio.samples, audio.sample_rate, &opts)?)
}

#[cfg(not(feature = "whisper"))]
fn transcribe_file(
    wav_path: &Path,
    model: &Path,
    lang: Option<&str>,
    glossary: &[String],
) -> Result<String> {
    let _ = (model, lang, glossary);
    // Still decode, so format errors are reported the same way.
    read_wav(wav_path)?;
    Err(CliError::Msg(
        "this keepsay-cli was built without the `whisper` feature; use --text-stdin",
    ))
}

fn current_month() -> YearMonth {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    YearMonth::from_unix_secs(secs)
}

fn config_path(dir: &Path) -> PathBuf {
    dir.join("config.json")
}

fn budget_path(dir: &Path) -> PathBuf {
    dir.join("budget.json")
}

fn models_dir(cli: &Cli) -> Result<PathBuf> {
    Ok(data_dir(cli)?.join("models"))
}

/// A missing settings file means defaults; nothing is created.
fn load_config(dir: &Path) -> Result<Config> {
    let p = config_path(dir);
    if p.exists() {
        Ok(Config::load(&p)?)
    } else {
        Ok(Config::default())
    }
}

fn load_budget(dir: &Path, now: YearMonth) -> Result<Budget> {
    let p = budget_path(dir);
    if p.exists() {
        Ok(Budget::load(&p, now)?)
    } else {
        Ok(Budget::new(now))
    }
}

/// `--data-dir`, else the platform's app data directory. The core never
/// picks this (docs/changes/0005 §2); the shell does.
fn data_dir(cli: &Cli) -> Result<PathBuf> {
    if let Some(d) = &cli.data_dir {
        return Ok(d.clone());
    }
    default_data_dir().ok_or(CliError::Msg(
        "cannot locate the app data directory; pass --data-dir",
    ))
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> Option<PathBuf> {
    Some(env_dir("HOME")?.join("Library/Application Support/dev.rightson.keepsay"))
}

#[cfg(windows)]
fn default_data_dir() -> Option<PathBuf> {
    Some(env_dir("APPDATA")?.join("dev.rightson.keepsay"))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn default_data_dir() -> Option<PathBuf> {
    let base = env_dir("XDG_DATA_HOME").or_else(|| Some(env_dir("HOME")?.join(".local/share")))?;
    Some(base.join("keepsay"))
}
