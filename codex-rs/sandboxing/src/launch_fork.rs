//! Fork: changes to a prepared command beyond upstream's shell snapshot replay.
//!
//! The exec-server shell snapshot replays a Bash command with `--norc`, because seccomp can make
//! Bash misdetect remote startup and re-read `.bashrc`.

use super::WrappedSandboxCommand;

impl WrappedSandboxCommand {
    /// Inserts `option` before the flag of the shell invocation `expected`, as Bash's `--norc`
    /// before `-pc` in a shell snapshot replay. Returns whether `expected` matched.
    pub fn insert_shell_option(&mut self, expected: &[String], option: String) -> bool {
        let Some(index) = self.shell_script_index(expected) else {
            return false;
        };
        self.request.command.insert(index - 1, option);
        self.inner_arg_count += 1;
        true
    }
}

#[cfg(test)]
#[path = "launch_fork_tests.rs"]
mod tests;
