use super::*;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use pretty_assertions::assert_eq;
use std::collections::HashMap;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

fn native(args: &[&str]) -> WrappedSandboxCommand {
    let cwd = AbsolutePathBuf::current_dir().expect("current dir");
    WrappedSandboxCommand::native(
        argv(args),
        PathUri::from_abs_path(&cwd),
        HashMap::new(),
        /*arg0*/ None,
    )
    .expect("native command")
}

#[test]
fn insert_shell_option_goes_before_the_shell_flag() {
    let replay = argv(&["/bin/bash", "-pc", "echo hi"]);
    let mut command = native(&["/bin/bash", "-pc", "echo hi"]);

    assert!(command.insert_shell_option(&replay, "--norc".to_string()));

    let with_option = argv(&["/bin/bash", "--norc", "-pc", "echo hi"]);
    // The inserted option counts as part of the submitted invocation.
    assert!(command.supports_shell_snapshot(&with_option));
    assert_eq!(
        command.into_command_request().expect("request").command,
        with_option
    );
}

#[test]
fn insert_shell_option_leaves_another_invocation_alone() {
    let mut command = native(&["/bin/bash", "-lc", "echo hi"]);

    assert!(!command.insert_shell_option(
        &argv(&["/bin/bash", "-pc", "echo hi"]),
        "--norc".to_string()
    ));

    assert_eq!(
        command.into_command_request().expect("request").command,
        argv(&["/bin/bash", "-lc", "echo hi"])
    );
}
