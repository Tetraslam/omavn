/// Progress and status events emitted by long operations. Frontends (the CLI,
/// a GUI, a JSON stream for agents) decide how to render them.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A new phase started, e.g. "extracting Dies Irae.part1.exe".
    Step {
        message: String,
    },
    /// Byte or item progress within the current step.
    Progress {
        done: u64,
        total: u64,
    },
    Info {
        message: String,
    },
    Warn {
        message: String,
    },
}

pub trait Reporter: Send + Sync {
    fn event(&self, event: Event);
}

/// Helpers usable through `&dyn Reporter`.
impl dyn Reporter + '_ {
    pub fn step(&self, message: impl Into<String>) {
        self.event(Event::Step {
            message: message.into(),
        });
    }
    pub fn info(&self, message: impl Into<String>) {
        self.event(Event::Info {
            message: message.into(),
        });
    }
    pub fn warn(&self, message: impl Into<String>) {
        self.event(Event::Warn {
            message: message.into(),
        });
    }
    pub fn progress(&self, done: u64, total: u64) {
        self.event(Event::Progress { done, total });
    }
}

/// Discards everything.
pub struct Silent;

impl Reporter for Silent {
    fn event(&self, _: Event) {}
}
