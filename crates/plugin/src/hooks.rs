/// Events that can trigger plugin hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HookEvent {
    /// Triggered when a session starts.
    SessionStart,
    /// Triggered when a file is saved.
    FileSave,
    /// Triggered when a session ends.
    SessionEnd,
}

impl HookEvent {
    /// Returns the string representation of this event.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::SessionStart => "session_start",
            Self::FileSave => "file_save",
            Self::SessionEnd => "session_end",
        }
    }

    /// Returns the script name for this event.
    #[must_use]
    pub const fn script_name(&self) -> &'static str {
        match *self {
            Self::SessionStart => "on-session-start.sh",
            Self::FileSave => "on-file-save.sh",
            Self::SessionEnd => "on-session-end.sh",
        }
    }
}

/// Context passed to hook scripts.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct HookContext {
    /// Session ID if available.
    pub session_id: Option<String>,
    /// Project path if available.
    pub project_path: Option<String>,
    /// File path if available.
    pub file_path: Option<String>,
    /// Editor name if available.
    pub editor: Option<String>,
}

impl HookContext {
    /// Creates a new empty hook context.
    #[must_use]
    pub const fn new() -> Self {
        Self { session_id: None, project_path: None, file_path: None, editor: None }
    }

    /// Sets the session ID.
    #[must_use]
    pub fn with_session_id<S: Into<String>>(mut self, id: S) -> Self {
        self.session_id = Some(id.into());
        self
    }

    /// Sets the project path.
    #[must_use]
    pub fn with_project_path<S: Into<String>>(mut self, path: S) -> Self {
        self.project_path = Some(path.into());
        self
    }

    /// Sets the file path.
    #[must_use]
    pub fn with_file_path<S: Into<String>>(mut self, path: S) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Sets the editor name.
    #[must_use]
    pub fn with_editor<S: Into<String>>(mut self, editor: S) -> Self {
        self.editor = Some(editor.into());
        self
    }
}
