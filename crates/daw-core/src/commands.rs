use std::error::Error;
use std::fmt;

/// The edit history is strictly control/UI-thread functionality, never audio-callback code.
pub trait Command<State>: Send {
    fn name(&self) -> &'static str;
    fn apply(&mut self, state: &mut State) -> Result<(), CommandError>;
    fn revert(&mut self, state: &mut State) -> Result<(), CommandError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandError(pub String);

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CommandError {}

pub struct UndoStack<State> {
    undo: Vec<Box<dyn Command<State>>>,
    redo: Vec<Box<dyn Command<State>>>,
    max_history: usize,
}

impl<State> UndoStack<State> {
    pub fn new(max_history: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_history,
        }
    }

    pub fn execute(
        &mut self,
        mut command: impl Command<State> + 'static,
        state: &mut State,
    ) -> Result<(), CommandError> {
        command.apply(state)?;
        self.redo.clear();
        if self.max_history > 0 {
            self.undo.push(Box::new(command));
            if self.undo.len() > self.max_history {
                self.undo.remove(0);
            }
        }
        Ok(())
    }

    pub fn undo(&mut self, state: &mut State) -> Result<bool, CommandError> {
        let Some(mut command) = self.undo.pop() else {
            return Ok(false);
        };
        if let Err(error) = command.revert(state) {
            self.undo.push(command);
            return Err(error);
        }
        self.redo.push(command);
        Ok(true)
    }

    pub fn redo(&mut self, state: &mut State) -> Result<bool, CommandError> {
        let Some(mut command) = self.redo.pop() else {
            return Ok(false);
        };
        if let Err(error) = command.apply(state) {
            self.redo.push(command);
            return Err(error);
        }
        self.undo.push(command);
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

impl<State> Default for UndoStack<State> {
    fn default() -> Self {
        Self::new(200)
    }
}

pub struct RenameProject {
    pub new_name: String,
    previous_name: Option<String>,
}

impl RenameProject {
    pub fn new(new_name: impl Into<String>) -> Self {
        Self {
            new_name: new_name.into(),
            previous_name: None,
        }
    }
}

impl Command<crate::model::Project> for RenameProject {
    fn name(&self) -> &'static str {
        "Rename project"
    }

    fn apply(&mut self, project: &mut crate::model::Project) -> Result<(), CommandError> {
        self.previous_name = Some(std::mem::replace(&mut project.name, self.new_name.clone()));
        Ok(())
    }

    fn revert(&mut self, project: &mut crate::model::Project) -> Result<(), CommandError> {
        let previous = self
            .previous_name
            .take()
            .ok_or_else(|| CommandError("cannot undo a rename that has not been applied".into()))?;
        project.name = previous;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;
    use crate::time::Tempo;

    #[test]
    fn undo_redo_and_redo_invalidation() {
        let mut project = Project::new("First", Tempo::default());
        let mut history = UndoStack::new(10);
        history
            .execute(RenameProject::new("Second"), &mut project)
            .unwrap();
        history
            .execute(RenameProject::new("Third"), &mut project)
            .unwrap();
        assert_eq!(project.name, "Third");
        assert_eq!(history.undo(&mut project), Ok(true));
        assert_eq!(project.name, "Second");
        assert_eq!(history.undo(&mut project), Ok(true));
        assert_eq!(project.name, "First");
        assert_eq!(history.undo(&mut project), Ok(false));
        assert_eq!(history.redo(&mut project), Ok(true));
        assert_eq!(project.name, "Second");
        history
            .execute(RenameProject::new("Alternative"), &mut project)
            .unwrap();
        assert!(!history.can_redo());
        assert_eq!(project.name, "Alternative");
    }

    #[test]
    fn bounded_history_drops_oldest_commands() {
        let mut project = Project::new("Zero", Tempo::default());
        let mut history = UndoStack::new(2);
        for new_name in ["One", "Two", "Three"] {
            history
                .execute(RenameProject::new(new_name), &mut project)
                .unwrap();
        }
        assert_eq!(history.undo_len(), 2);
        history.undo(&mut project).unwrap();
        history.undo(&mut project).unwrap();
        assert_eq!(project.name, "One");
    }

    #[test]
    fn zero_history_still_executes_changes() {
        let mut project = Project::new("Before", Tempo::default());
        let mut history = UndoStack::new(0);
        history
            .execute(RenameProject::new("After"), &mut project)
            .unwrap();
        assert_eq!(project.name, "After");
        assert!(!history.can_undo());
    }
}
