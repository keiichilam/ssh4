//! Terminal protocol logic shared by CLI and GUI. Everything in this module
//! must be unit-testable without an SSH server or a UI.

pub mod brackets;
pub mod input;
pub mod mouse;
pub mod output;
pub mod selection;
