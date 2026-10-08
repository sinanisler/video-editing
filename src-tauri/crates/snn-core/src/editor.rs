use crate::command::{apply, Command};
use crate::model::{Project, PROJECT_VERSION};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const MAX_UNDO: usize = 200;
const MAX_LOG: usize = 200;

struct Step {
    snapshot: Project,
    tool: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    pub seq: u64,
    pub actor: String,
    pub tool: String,
    pub args: Value,
    pub ok: bool,
    pub result: Value,
}

/// What the GUI renders. Sent whole after every change.
#[derive(Clone, Debug, Serialize)]
pub struct EditorState {
    pub project: Project,
    pub path: Option<String>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub history_len: usize,
    /// Newest first.
    pub log: Vec<LogEntry>,
}

/// One open project with its undo history and activity log.
pub struct Editor {
    project: Project,
    undo: Vec<Step>,
    redo: Vec<Step>,
    log: Vec<LogEntry>,
    seq: u64,
    path: Option<PathBuf>,
    dirty: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(Project::new("Untitled"))
    }
}

impl Editor {
    pub fn new(project: Project) -> Self {
        Editor { project, undo: vec![], redo: vec![], log: vec![], seq: 0, path: None, dirty: false }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    pub(crate) fn record(&mut self, actor: &str, tool: String, args: Value, result: &Result<Value, String>) {
        self.seq += 1;
        let (ok, result) = match result {
            Ok(v) => (true, v.clone()),
            Err(e) => (false, Value::String(e.clone())),
        };
        self.log.insert(0, LogEntry { seq: self.seq, actor: actor.into(), tool, args, ok, result });
        self.log.truncate(MAX_LOG);
    }

    /// Runs one command for `actor` (`user` or an agent name). A failed command leaves the project untouched.
    pub fn execute(&mut self, actor: &str, cmd: Command) -> Result<Value, String> {
        let before = self.project.clone();
        let result = apply(&mut self.project, &cmd, actor);
        match &result {
            Ok(_) => {
                self.undo.push(Step { snapshot: before, tool: cmd.tool() });
                if self.undo.len() > MAX_UNDO {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
                if matches!(cmd, Command::NewProject { .. }) {
                    self.path = None;
                }
            }
            Err(_) => self.project = before,
        }
        self.record(actor, cmd.tool(), cmd.args(), &result);
        result
    }

    /// Runs several commands as one undo step. If one fails, none is applied.
    pub fn execute_batch(&mut self, actor: &str, cmds: Vec<Command>) -> Result<Value, String> {
        let before = self.project.clone();
        let args = serde_json::json!({ "commands": cmds.iter().map(Command::tool).collect::<Vec<_>>() });
        let mut results = vec![];
        let mut failure = None;
        for (i, cmd) in cmds.iter().enumerate() {
            match apply(&mut self.project, cmd, actor) {
                Ok(v) => results.push(v),
                Err(e) => {
                    failure = Some(format!("command {i} ({}) failed: {e}; nothing was changed", cmd.tool()));
                    break;
                }
            }
        }
        let result = match failure {
            Some(e) => {
                self.project = before;
                Err(e)
            }
            None => {
                self.undo.push(Step { snapshot: before, tool: "batch.apply".into() });
                if self.undo.len() > MAX_UNDO {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
                Ok(serde_json::json!({ "results": results }))
            }
        };
        self.record(actor, "batch.apply".into(), args, &result);
        result
    }

    pub fn undo(&mut self, actor: &str) -> Result<Value, String> {
        let result = match self.undo.pop() {
            Some(step) => {
                let current = std::mem::replace(&mut self.project, step.snapshot);
                self.redo.push(Step { snapshot: current, tool: step.tool.clone() });
                self.dirty = true;
                Ok(serde_json::json!({ "undone": step.tool }))
            }
            None => Err("nothing to undo".to_string()),
        };
        self.record(actor, "history.undo".into(), serde_json::json!({}), &result);
        result
    }

    pub fn redo(&mut self, actor: &str) -> Result<Value, String> {
        let result = match self.redo.pop() {
            Some(step) => {
                let current = std::mem::replace(&mut self.project, step.snapshot);
                self.undo.push(Step { snapshot: current, tool: step.tool.clone() });
                self.dirty = true;
                Ok(serde_json::json!({ "redone": step.tool }))
            }
            None => Err("nothing to redo".to_string()),
        };
        self.record(actor, "history.redo".into(), serde_json::json!({}), &result);
        result
    }

    /// Saves to `path`, or to the path the project was opened from or last saved to.
    pub fn save(&mut self, path: Option<&Path>) -> Result<String, String> {
        let path = path
            .map(Path::to_path_buf)
            .or_else(|| self.path.clone())
            .ok_or("the project has no file yet; choose where to save it")?;
        let json = serde_json::to_string_pretty(&self.project).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("could not write {}: {e}", path.display()))?;
        self.path = Some(path.clone());
        self.dirty = false;
        Ok(path.display().to_string())
    }

    /// Replaces the open project with the one in `path`. Clears the undo history.
    pub fn open(&mut self, path: &Path) -> Result<String, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let mut project: Project =
            serde_json::from_str(&text).map_err(|e| format!("{} is not a valid project: {e}", path.display()))?;
        if project.version > PROJECT_VERSION {
            return Err(format!(
                "project format {} is newer than this app understands ({PROJECT_VERSION})",
                project.version
            ));
        }
        project.link_legacy_pairs();
        self.project = project;
        self.undo.clear();
        self.redo.clear();
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(path.display().to_string())
    }

    pub fn state(&self) -> EditorState {
        EditorState {
            project: self.project.clone(),
            path: self.path.as_ref().map(|p| p.display().to_string()),
            dirty: self.dirty,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            undo_label: self.undo.last().map(|s| s.tool.clone()),
            redo_label: self.redo.last().map(|s| s.tool.clone()),
            history_len: self.undo.len(),
            log: self.log.clone(),
        }
    }
}
