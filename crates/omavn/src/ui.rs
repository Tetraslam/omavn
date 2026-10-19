use std::io::{IsTerminal, Write};

/// `println!` that exits quietly when stdout is closed (`omavn list | head`)
/// instead of panicking.
macro_rules! out {
    ($($arg:tt)*) => {{
        let mut stdout = std::io::stdout().lock();
        if writeln!(stdout, $($arg)*).is_err() {
            std::process::exit(0);
        }
    }};
}
use std::sync::Mutex;

use indicatif::{ProgressBar, ProgressStyle};
use omavn_core::{Event, Reporter};
use owo_colors::OwoColorize;

/// Renders core events and command results, either for humans or as JSON.
pub struct Ui {
    json: bool,
    color: bool,
    bar: Mutex<Option<ProgressBar>>,
}

impl Ui {
    pub fn new(json: bool) -> Self {
        let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        Self {
            json,
            color,
            bar: Mutex::new(None),
        }
    }

    /// Print a command's result: JSON on stdout, or run `human`.
    pub fn result<T: serde::Serialize>(&self, value: &T, human: impl FnOnce()) {
        if self.json {
            out!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        } else {
            human();
        }
    }

    pub fn line(&self, s: &str) {
        out!("{s}");
    }

    pub fn dim(&self, s: &str) -> String {
        if self.color {
            s.dimmed().to_string()
        } else {
            s.to_string()
        }
    }

    pub fn success(&self, s: &str) {
        if self.color {
            out!("{} {s}", "✓".green());
        } else {
            out!("{s}");
        }
    }

    pub fn check(&self, ok: bool, required: bool, name: &str, detail: &str) {
        let mark = match (ok, required, self.color) {
            (true, _, true) => "✓".green().to_string(),
            (false, true, true) => "✗".red().to_string(),
            (false, false, true) => "!".yellow().to_string(),
            (true, _, false) => "ok".into(),
            (false, true, false) => "FAIL".into(),
            (false, false, false) => "warn".into(),
        };
        out!("{mark} {name:<14} {detail}");
    }

    pub fn error(&self, e: &anyhow::Error) {
        self.finish_progress();
        if self.json {
            let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
            eprintln!(
                "{}",
                serde_json::json!({ "event": "error", "message": e.to_string(), "causes": chain })
            );
            return;
        }
        let head = if self.color {
            "error:".red().bold().to_string()
        } else {
            "error:".into()
        };
        eprintln!("{head} {e}");
        for cause in e.chain().skip(1) {
            eprintln!("  {} {cause}", self.dim("caused by:"));
        }
        eprintln!(
            "{}",
            self.dim(
                "stuck? `omavn feedback --kind bug \"what happened\"` files a prefilled report"
            )
        );
    }

    pub fn confirm(&self, question: &str) -> anyhow::Result<bool> {
        if self.json || !std::io::stdin().is_terminal() {
            return Ok(false);
        }
        eprint!("{question} [y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
    }

    pub fn finish_progress(&self) {
        if let Some(bar) = self.bar.lock().unwrap().take() {
            bar.finish_and_clear();
        }
    }
}

impl Reporter for Ui {
    fn event(&self, event: Event) {
        if self.json {
            eprintln!("{}", serde_json::to_string(&event).unwrap_or_default());
            return;
        }
        let mut bar = self.bar.lock().unwrap();
        match event {
            Event::Step { message } => {
                if let Some(b) = bar.take() {
                    b.finish_and_clear();
                }
                let arrow = if self.color {
                    "→".cyan().to_string()
                } else {
                    "->".into()
                };
                eprintln!("{arrow} {message}");
            }
            Event::Progress { done, total } => {
                if total == 0 || !std::io::stderr().is_terminal() {
                    return;
                }
                let b = bar.get_or_insert_with(|| {
                    let b = ProgressBar::new(total);
                    b.set_style(
                        ProgressStyle::with_template(
                            "  {bar:30.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec} eta {eta}",
                        )
                        .unwrap()
                        .progress_chars("━╸ "),
                    );
                    b
                });
                b.set_length(total);
                b.set_position(done);
            }
            Event::Info { message } => {
                let msg = format!("  {message}");
                match bar.as_ref() {
                    Some(b) => b.println(msg),
                    None => eprintln!("{msg}"),
                }
            }
            Event::Warn { message } => {
                let head = if self.color {
                    "warning:".yellow().to_string()
                } else {
                    "warning:".into()
                };
                let msg = format!("{head} {message}");
                match bar.as_ref() {
                    Some(b) => b.println(msg),
                    None => eprintln!("{msg}"),
                }
            }
        }
    }
}
