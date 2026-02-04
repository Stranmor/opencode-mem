#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    SessionStart,
    FileSave,
    SessionEnd,
}

impl HookEvent {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::SessionStart => "session_start",
            Self::FileSave => "file_save",
            Self::SessionEnd => "session_end",
        }
    }

    #[must_use]
    pub const fn script_name(&self) -> &'static str {
        match *self {
            Self::SessionStart => "on-session-start.sh",
            Self::FileSave => "on-file-save.sh",
            Self::SessionEnd => "on-session-end.sh",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    pub file_path: Option<String>,
    pub editor: Option<String>,
}

impl HookContext {
    #[must_use]
    pub const fn new() -> Self {
        Self { session_id: None, project_path: None, file_path: None, editor: None }
    }

    #[must_use]
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn with_project_path(mut self, path: impl Into<String>) -> Self {
        self.project_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_editor(mut self, editor: impl Into<String>) -> Self {
        self.editor = Some(editor.into());
        self
    }
}
