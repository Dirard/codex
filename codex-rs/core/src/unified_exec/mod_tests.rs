use super::process::OutputDrainPolicy;
use super::*;
use crate::codex_thread::BackgroundTerminalInfo;
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecExpiration;
use crate::sandboxing::ExecRequest;
use crate::sandboxing::SandboxPermissions;
use crate::session::session::Session;
use crate::session::tests::make_session_and_context;
use crate::session::turn_context::TurnContext;
use crate::tools::context::ExecCommandToolOutput;
use crate::unified_exec::WriteStdinRequest;
use crate::unified_exec::output::CapturedStream;
use codex_exec_server::ExecProcess;
use codex_exec_server::ExecProcessEventReceiver;
use codex_exec_server::ExecProcessFuture;
use codex_exec_server::ProcessId;
use codex_exec_server::ProcessSignal;
use codex_exec_server::ReadResponse;
use codex_exec_server::StartedExecProcess;
use codex_exec_server::WriteResponse;
use codex_exec_server::WriteStatus;
use codex_sandboxing::SandboxType;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::approx_tokens_from_byte_count;
use core_test_support::skip_if_no_remote_env;
use core_test_support::skip_if_sandbox;
use core_test_support::test_codex::test_env as remote_test_env;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::sync::watch;
use tokio::time::Duration;
use tokio::time::Instant;

async fn test_session_and_turn() -> (Arc<Session>, Arc<TurnContext>) {
    let (session, turn) = make_session_and_context().await;
    (Arc::new(session), Arc::new(turn))
}

async fn exec_command(
    session: &Arc<Session>,
    turn: &Arc<TurnContext>,
    cmd: &str,
    yield_time_ms: u64,
    workdir: Option<PathBuf>,
) -> Result<ExecCommandToolOutput, UnifiedExecError> {
    exec_command_with_tty(
        session,
        turn,
        cmd,
        yield_time_ms,
        workdir,
        /*tty*/ true,
    )
    .await
}

fn shell_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn test_exec_request(
    turn: &TurnContext,
    command: Vec<String>,
    cwd: AbsolutePathBuf,
    env: HashMap<String, String>,
) -> ExecRequest {
    let permission_profile = turn.permission_profile();
    let network = None;
    let arg0 = None;
    ExecRequest::new(
        command,
        cwd,
        env,
        network,
        /*network_environment_id*/ None,
        ExecExpiration::DefaultTimeout,
        ExecCapturePolicy::ShellTool,
        SandboxType::None,
        turn.config
            .effective_workspace_roots()
            .iter()
            .map(PathUri::to_abs_path)
            .collect::<std::io::Result<Vec<_>>>()
            .expect("test workspace roots are host-native"),
        turn.windows_sandbox_level,
        permission_profile,
        arg0,
    )
}

async fn exec_command_with_tty(
    session: &Arc<Session>,
    turn: &Arc<TurnContext>,
    cmd: &str,
    yield_time_ms: u64,
    workdir: Option<PathBuf>,
    tty: bool,
) -> Result<ExecCommandToolOutput, UnifiedExecError> {
    let manager = &session.services.unified_exec_manager;
    let process_id = manager.allocate_process_id().await;
    #[allow(deprecated)]
    let cwd = workdir
        .as_ref()
        .map_or_else(|| turn.cwd.clone(), |workdir| turn.cwd.join(workdir));
    let command = vec!["bash".to_string(), "-lc".to_string(), cmd.to_string()];
    let request = test_exec_request(turn, command.clone(), cwd.clone(), shell_env());

    let process = Arc::new(
        manager
            .open_session_with_prepared_exec_env(
                process_id,
                &request,
                /*tool_ctx*/ None,
                codex_sandboxing::WindowsSandboxProxySettingsMode::Reconcile,
                /*network_policy_decider*/ None,
                tty,
                turn.initial_environments
                    .primary()
                    .expect("turn environment")
                    .environment
                    .as_ref(),
            )
            .await?,
    );
    let context = UnifiedExecContext::new(
        Arc::clone(session),
        crate::session::step_context::StepContext::for_test(Arc::clone(turn)),
        tokio_util::sync::CancellationToken::new(),
        "call".to_string(),
    );
    let started_at = Instant::now();
    let process_started_alive = !process.has_exited() && process.exit_code().is_none();
    if process_started_alive {
        let entry = ProcessEntry {
            process: Arc::clone(&process),
            plugin_metrics_sidecar: None,
            call_id: context.call_id.clone(),
            process_id,
            cwd: cwd.clone().into(),
            initial_exec_command_active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            hook_command: cmd.to_string(),
            tty,
            environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
            permissions: TerminalPermissions::for_launch(
                turn.initial_environments
                    .primary()
                    .expect("turn environment"),
                turn,
                TerminalSandboxSource::Native,
                SandboxPermissions::UseDefault,
                /*additional_permissions*/ None,
                /*internal_permissions*/ None,
            ),
            network_approval: None,
            session: Arc::downgrade(session),
            last_used: started_at,
            exit_notifier: None,
        };
        manager
            .process_store
            .lock()
            .await
            .processes
            .insert(process_id, entry);
    }

    let deadline = started_at + Duration::from_millis(yield_time_ms);
    let collected_output = UnifiedExecProcessManager::collect_output_until_deadline(
        process.output_handles(),
        Some(session.subscribe_elicitation_pause_state()),
        deadline,
    )
    .await;
    let wall_time = Instant::now().saturating_duration_since(started_at);
    let original_token_count = usize::try_from(approx_tokens_from_byte_count(
        collected_output.total_bytes(),
    ))
    .unwrap_or(usize::MAX);
    let collected = CapturedStream::from(&collected_output);
    let has_exited = process.has_exited();
    let exit_code = process.exit_code();
    let response_process_id = if process_started_alive && !has_exited {
        Some(process_id)
    } else {
        manager.release_process_id(process_id).await;
        None
    };
    if response_process_id.is_some()
        && let Some(entry) = manager
            .process_store
            .lock()
            .await
            .processes
            .get_mut(&process_id)
    {
        entry
            .initial_exec_command_active
            .store(false, std::sync::atomic::Ordering::Release);
    }

    Ok(ExecCommandToolOutput {
        event_call_id: context.call_id,
        chunk_id: generate_chunk_id(),
        wall_time,
        raw_output: collected,
        truncation: turn.output_truncation(),
        max_output_tokens: None,
        process_id: response_process_id,
        exit_code,
        original_token_count: Some(original_token_count),
        hook_command: Some(cmd.to_string()),
        tool_cwd: None,
        armed_notifications: Default::default(),
    })
}

struct BlockingTerminateExecProcess {
    process_id: ProcessId,
    terminate_started: watch::Sender<bool>,
    allow_terminate: Arc<Notify>,
    wake_tx: watch::Sender<u64>,
}

impl BlockingTerminateExecProcess {
    async fn read(&self) -> Result<ReadResponse, codex_exec_server::ExecServerError> {
        Ok(ReadResponse {
            chunks: Vec::new(),
            next_seq: 1,
            exited: false,
            exit_code: None,
            closed: false,
            failure: None,
            sandbox_denied: false,
        })
    }

    async fn write(&self) -> Result<WriteResponse, codex_exec_server::ExecServerError> {
        Ok(WriteResponse {
            status: WriteStatus::Accepted,
        })
    }

    async fn terminate(&self) -> Result<(), codex_exec_server::ExecServerError> {
        let _ = self.terminate_started.send(true);
        self.allow_terminate.notified().await;
        Ok(())
    }
}

impl ExecProcess for BlockingTerminateExecProcess {
    fn process_id(&self) -> &ProcessId {
        &self.process_id
    }

    fn subscribe_wake(&self) -> watch::Receiver<u64> {
        self.wake_tx.subscribe()
    }

    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        ExecProcessEventReceiver::empty()
    }

    fn read(
        &self,
        _after_seq: Option<u64>,
        _max_bytes: Option<usize>,
        _wait_ms: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        Box::pin(BlockingTerminateExecProcess::read(self))
    }

    fn write(&self, _chunk: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        Box::pin(BlockingTerminateExecProcess::write(self))
    }

    fn signal(&self, _signal: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        Box::pin(BlockingTerminateExecProcess::terminate(self))
    }
}

async fn blocking_terminate_unified_process(
    process_id: i32,
    terminate_started: watch::Sender<bool>,
    allow_terminate: Arc<Notify>,
) -> anyhow::Result<Arc<UnifiedExecProcess>> {
    let (wake_tx, _wake_rx) = watch::channel(0);
    Ok(Arc::new(
        UnifiedExecProcess::from_exec_server_started(
            StartedExecProcess {
                process: Arc::new(BlockingTerminateExecProcess {
                    process_id: process_id.to_string().into(),
                    terminate_started,
                    allow_terminate,
                    wake_tx,
                }),
                sandbox_type: Some(codex_sandboxing::SandboxType::None),
            },
            OutputDrainPolicy::WaitForOutputClosure,
        )
        .await?,
    ))
}

async fn write_stdin(
    session: &Arc<Session>,
    turn: &Arc<TurnContext>,
    process_id: i32,
    input: &str,
    yield_time_ms: u64,
) -> Result<ExecCommandToolOutput, UnifiedExecError> {
    let manager = &session.services.unified_exec_manager;
    let terminal = manager.resolve_terminal(process_id).await?;
    manager
        .write_stdin(
            &UnifiedExecContext::new(
                Arc::clone(session),
                crate::session::step_context::StepContext::for_test(Arc::clone(turn)),
                tokio_util::sync::CancellationToken::new(),
                "write".to_string(),
            ),
            WriteStdinRequest {
                terminal: &terminal,
                input,
                yield_time_ms,
                max_output_tokens: None,
                truncation: OutputTruncation::new(TruncationPolicy::Tokens(10_000), None),
                interaction_event: None,
                notifications: Default::default(),
            },
        )
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_persists_across_requests() -> anyhow::Result<()> {
    skip_if_sandbox!(Ok(()));

    let (session, turn) = test_session_and_turn().await;
    #[allow(deprecated)]
    let cwd = turn.cwd.clone();

    let open_shell = exec_command(
        &session, &turn, "bash -i", /*yield_time_ms*/ 2_500, /*workdir*/ None,
    )
    .await?;
    let process_id = open_shell.process_id.expect("expected process_id");
    assert_eq!(
        session.list_background_terminals().await,
        vec![BackgroundTerminalInfo {
            item_id: "call".to_string(),
            process_id: process_id.to_string(),
            command: "bash -i".to_string(),
            cwd: cwd.into(),
        }]
    );

    write_stdin(
        &session,
        &turn,
        process_id,
        "export CODEX_INTERACTIVE_SHELL_VAR=codex\n",
        /*yield_time_ms*/ 2_500,
    )
    .await?;

    let out_2 = write_stdin(
        &session,
        &turn,
        process_id,
        "echo $CODEX_INTERACTIVE_SHELL_VAR\n",
        /*yield_time_ms*/ 2_500,
    )
    .await?;
    assert!(
        out_2
            .truncated_output(DEFAULT_MAX_OUTPUT_TOKENS)
            .contains("codex"),
        "expected environment variable output"
    );

    assert!(session.terminate_background_terminal(process_id).await);
    assert!(!session.terminate_background_terminal(process_id).await);
    assert!(session.list_background_terminals().await.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_unified_exec_sessions() -> anyhow::Result<()> {
    skip_if_sandbox!(Ok(()));

    let (session, turn) = test_session_and_turn().await;

    let shell_a = exec_command(
        &session, &turn, "bash -i", /*yield_time_ms*/ 2_500, /*workdir*/ None,
    )
    .await?;
    let session_a = shell_a.process_id.expect("expected process id");

    write_stdin(
        &session,
        &turn,
        session_a,
        "export CODEX_INTERACTIVE_SHELL_VAR=codex\n",
        /*yield_time_ms*/ 2_500,
    )
    .await?;

    let out_2 = exec_command(
        &session,
        &turn,
        "echo $CODEX_INTERACTIVE_SHELL_VAR",
        /*yield_time_ms*/ 2_500,
        /*workdir*/ None,
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        out_2.process_id.is_none(),
        "short command should not report a process id if it exits quickly"
    );
    assert!(
        !out_2
            .truncated_output(DEFAULT_MAX_OUTPUT_TOKENS)
            .contains("codex"),
        "short command should run in a fresh shell"
    );

    let out_3 = write_stdin(
        &session,
        &turn,
        shell_a.process_id.expect("expected process id"),
        "echo $CODEX_INTERACTIVE_SHELL_VAR\n",
        /*yield_time_ms*/ 2_500,
    )
    .await?;
    assert!(
        out_3
            .truncated_output(DEFAULT_MAX_OUTPUT_TOKENS)
            .contains("codex"),
        "session should preserve state"
    );

    Ok(())
}

#[tokio::test]
async fn unified_exec_timeouts() -> anyhow::Result<()> {
    skip_if_sandbox!(Ok(()));

    const TEST_VAR_VALUE: &str = "unified_exec_var_123";

    let (session, turn) = test_session_and_turn().await;

    let open_shell = exec_command(
        &session, &turn, "bash -i", /*yield_time_ms*/ 2_500, /*workdir*/ None,
    )
    .await?;
    let process_id = open_shell.process_id.expect("expected process id");

    write_stdin(
        &session,
        &turn,
        process_id,
        format!("export CODEX_INTERACTIVE_SHELL_VAR={TEST_VAR_VALUE}\n").as_str(),
        /*yield_time_ms*/ 2_500,
    )
    .await?;

    let out_2 = write_stdin(
        &session,
        &turn,
        process_id,
        "sleep 5 && echo $CODEX_INTERACTIVE_SHELL_VAR\n",
        /*yield_time_ms*/ 10,
    )
    .await?;
    assert!(
        !out_2
            .truncated_output(DEFAULT_MAX_OUTPUT_TOKENS)
            .contains(TEST_VAR_VALUE),
        "timeout too short should yield incomplete output"
    );

    tokio::time::sleep(Duration::from_secs(7)).await;

    let out_3 = write_stdin(&session, &turn, process_id, "", /*yield_time_ms*/ 100).await?;

    assert!(
        out_3
            .truncated_output(DEFAULT_MAX_OUTPUT_TOKENS)
            .contains(TEST_VAR_VALUE),
        "subsequent poll should retrieve output"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_pause_blocks_yield_timeout() -> anyhow::Result<()> {
    skip_if_sandbox!(Ok(()));

    let (session, turn) = test_session_and_turn().await;
    let elicitation = session.services.elicitations.register();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        drop(elicitation);
    });

    let started = tokio::time::Instant::now();
    let response = exec_command(
        &session,
        &turn,
        "sleep 1 && echo unified-exec-done",
        /*yield_time_ms*/ 250,
        /*workdir*/ None,
    )
    .await?;

    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "pause should block the unified exec yield timeout"
    );
    assert!(
        response
            .truncated_output(DEFAULT_MAX_OUTPUT_TOKENS)
            .contains("unified-exec-done"),
        "exec_command should wait for output after the pause lifts"
    );
    assert!(
        response.process_id.is_none(),
        "completed command should not leave a background process"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reusing_completed_process_returns_unknown_process() -> anyhow::Result<()> {
    skip_if_sandbox!(Ok(()));

    let (session, turn) = test_session_and_turn().await;

    let open_shell = exec_command(
        &session, &turn, "bash -i", /*yield_time_ms*/ 2_500, /*workdir*/ None,
    )
    .await?;
    let process_id = open_shell.process_id.expect("expected process id");

    tokio::time::timeout(Duration::from_secs(/*secs*/ 10), async {
        let mut response = write_stdin(
            &session, &turn, process_id, "exit\n", /*yield_time_ms*/ 2_500,
        )
        .await?;
        while response.process_id.is_some() {
            response = write_stdin(&session, &turn, process_id, "", /*yield_time_ms*/ 100).await?;
        }
        anyhow::Ok(())
    })
    .await??;

    let err = write_stdin(&session, &turn, process_id, "", /*yield_time_ms*/ 100)
        .await
        .expect_err("expected unknown process error");

    match err {
        UnifiedExecError::UnknownProcessId { process_id: err_id } => {
            assert_eq!(err_id, process_id, "process id should match request");
        }
        other => panic!("expected UnknownProcessId, got {other:?}"),
    }

    assert!(
        session
            .services
            .unified_exec_manager
            .process_store
            .lock()
            .await
            .processes
            .is_empty()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminating_initial_exec_command_rechecks_initial_response_state() -> anyhow::Result<()> {
    let (session, turn) = test_session_and_turn().await;
    let manager = &session.services.unified_exec_manager;
    let process_id = manager.allocate_process_id().await;
    let (terminate_started_tx, mut terminate_started_rx) = watch::channel(false);
    let allow_terminate = Arc::new(Notify::new());
    let process = blocking_terminate_unified_process(
        process_id,
        terminate_started_tx,
        Arc::clone(&allow_terminate),
    )
    .await?;
    #[allow(deprecated)]
    let cwd = turn.cwd.clone();
    manager.process_store.lock().await.processes.insert(
        process_id,
        ProcessEntry {
            process,
            plugin_metrics_sidecar: None,
            call_id: "call".to_string(),
            process_id,
            cwd: cwd.into(),
            initial_exec_command_active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            hook_command: "sleep 60".to_string(),
            tty: true,
            environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
            permissions: TerminalPermissions::for_launch(
                turn.initial_environments
                    .primary()
                    .expect("turn environment"),
                &turn,
                TerminalSandboxSource::Native,
                SandboxPermissions::UseDefault,
                /*additional_permissions*/ None,
                /*internal_permissions*/ None,
            ),
            network_approval: None,
            session: Arc::downgrade(&session),
            exit_notifier: None,
            last_used: Instant::now(),
        },
    );

    let terminate_task = tokio::spawn({
        let session = Arc::clone(&session);
        async move { session.terminate_background_terminal(process_id).await }
    });
    tokio::time::timeout(
        Duration::from_secs(2),
        terminate_started_rx.wait_for(|started| *started),
    )
    .await
    .expect("terminate should start")
    .expect("terminate signal sender should stay open");

    {
        let mut store = manager.process_store.lock().await;
        let entry = store
            .processes
            .get_mut(&process_id)
            .expect("process should remain stored until initial response returns");
        entry
            .initial_exec_command_active
            .store(false, std::sync::atomic::Ordering::Release);
    }

    // Retain the release even if the sole termination waiter has not registered yet.
    allow_terminate.notify_one();
    let terminated = tokio::time::timeout(Duration::from_secs(2), terminate_task)
        .await
        .expect("terminate should finish")
        .expect("terminate task should not panic");
    assert!(terminated);
    assert!(
        !manager
            .process_store
            .lock()
            .await
            .processes
            .contains_key(&process_id)
    );

    Ok(())
}

#[test_case::test_case(false; "terminal removed")]
#[test_case::test_case(true; "session id reused")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminating_during_stdin_poll_returns_exited_response(
    reuse_id: bool,
) -> anyhow::Result<()> {
    let (session, turn) = test_session_and_turn().await;
    let manager = &session.services.unified_exec_manager;
    let process_id = manager.allocate_process_id().await;
    let (terminate_started_tx, _terminate_started_rx) = watch::channel(false);
    let allow_terminate = Arc::new(Notify::new());
    let process = blocking_terminate_unified_process(
        process_id,
        terminate_started_tx,
        Arc::clone(&allow_terminate),
    )
    .await?;
    #[allow(deprecated)]
    let cwd = turn.cwd.clone();
    let last_used = Instant::now() - Duration::from_secs(1);
    manager.process_store.lock().await.processes.insert(
        process_id,
        ProcessEntry {
            process: Arc::clone(&process),
            plugin_metrics_sidecar: None,
            call_id: "call".to_string(),
            process_id,
            cwd: cwd.into(),
            initial_exec_command_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            hook_command: "sleep 60".to_string(),
            tty: true,
            environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
            permissions: TerminalPermissions::for_launch(
                turn.initial_environments
                    .primary()
                    .expect("turn environment"),
                &turn,
                TerminalSandboxSource::Native,
                SandboxPermissions::UseDefault,
                /*additional_permissions*/ None,
                /*internal_permissions*/ None,
            ),
            network_approval: None,
            session: Arc::downgrade(&session),
            exit_notifier: None,
            last_used,
        },
    );

    let poll_task = tokio::spawn({
        let session = Arc::clone(&session);
        let turn = Arc::clone(&turn);
        async move {
            write_stdin(
                &session, &turn, process_id, "", /*yield_time_ms*/ 60_000,
            )
            .await
        }
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let poll_started = manager
                .process_store
                .lock()
                .await
                .processes
                .get(&process_id)
                .is_some_and(|entry| entry.last_used != last_used);
            if poll_started {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("poll should clone process handles");

    if reuse_id {
        let replacement = super::process_tests::remote_process(
            WriteStatus::Accepted,
            /*terminate_error*/ None,
            SandboxType::None,
        )
        .await;
        let mut store = manager.process_store.lock().await;
        let entry = store.processes.get_mut(&process_id).unwrap();
        entry.process = Arc::new(replacement);
        entry.call_id = "replacement".into();
    } else {
        manager.release_process_id(process_id).await;
    }
    allow_terminate.notify_one();
    process.terminate_confirmed().await?;

    let output = tokio::time::timeout(Duration::from_secs(2), poll_task)
        .await
        .expect("poll should finish")
        .expect("poll task should not panic")?;
    assert_eq!(
        (output.process_id, output.event_call_id.as_str()),
        (None, "call")
    );
    assert_eq!(
        manager
            .process_store
            .lock()
            .await
            .processes
            .contains_key(&process_id),
        reuse_id
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_pipe_commands_preserve_exit_code() -> anyhow::Result<()> {
    let (_, turn) = make_session_and_context().await;
    #[allow(deprecated)]
    let cwd = turn.cwd.clone();
    let request = test_exec_request(
        &turn,
        vec!["bash".to_string(), "-lc".to_string(), "exit 17".to_string()],
        cwd,
        shell_env(),
    );

    let environment = codex_exec_server::Environment::default_for_tests();
    let process = UnifiedExecProcessManager::default()
        .open_session_with_prepared_exec_env(
            /*process_id*/ 1234,
            &request,
            /*tool_ctx*/ None,
            codex_sandboxing::WindowsSandboxProxySettingsMode::Reconcile,
            /*network_policy_decider*/ None,
            /*tty*/ false,
            &environment,
        )
        .await?;

    if !process.has_exited() {
        let exit_signal = process.cancellation_token();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), exit_signal.cancelled())
                .await
                .is_ok(),
            "process did not report exit within timeout"
        );
    }

    assert!(process.has_exited());
    assert_eq!(process.exit_code(), Some(17));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_uses_remote_exec_server_when_configured() -> anyhow::Result<()> {
    skip_if_sandbox!(Ok(()));
    skip_if_no_remote_env!(Ok(()));

    let remote_test_env = remote_test_env().await?;
    let (_, turn) = make_session_and_context().await;
    let request = test_exec_request(
        &turn,
        vec!["bash".to_string(), "-i".to_string()],
        remote_test_env.cwd().clone(),
        shell_env(),
    );

    let manager = UnifiedExecProcessManager::default();
    let process = manager
        .open_session_with_prepared_exec_env(
            /*process_id*/ 1234,
            &request,
            /*tool_ctx*/ None,
            codex_sandboxing::WindowsSandboxProxySettingsMode::Reconcile,
            /*network_policy_decider*/ None,
            /*tty*/ true,
            remote_test_env.environment(),
        )
        .await?;

    process.write(b"printf 'remote-unified-exec\\n'\n").await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let collected = UnifiedExecProcessManager::collect_output_until_deadline(
        process.output_handles(),
        /*pause_state*/ None,
        Instant::now() + Duration::from_millis(2_500),
    )
    .await
    .to_bytes_with_omission_marker();

    assert!(String::from_utf8_lossy(&collected).contains("remote-unified-exec"));
    Ok(())
}

#[tokio::test]
async fn stdin_approval_preserves_the_reviewed_terminal() -> anyhow::Result<()> {
    use crate::session::tests::make_session_and_context_with_auth_and_config_and_rx;
    use crate::state::ActiveTurn;
    use crate::tools::sandboxing::ToolError;
    use codex_features::Feature;
    use codex_protocol::config_types::ApprovalsReviewer;
    use codex_protocol::protocol::AskForApproval;
    use codex_protocol::protocol::EventMsg;
    use codex_protocol::protocol::ReviewDecision;

    skip_if_sandbox!(Ok(()));
    let (session, mut turn, events) = make_session_and_context_with_auth_and_config_and_rx(
        codex_login::CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        |config| {
            config.features.enable(Feature::WriteStdinApproval).unwrap();
            config.permissions.approval_policy =
                crate::config::Constrained::allow_any(AskForApproval::OnRequest);
            config.approvals_reviewer = ApprovalsReviewer::AutoReview;
        },
    )
    .await;
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    let manager = &session.services.unified_exec_manager;
    let command = "while IFS= read -r line; do printf 'received:%s\\n' \"$line\"; done";
    let opened = exec_command(
        &session, &turn, command, /*yield_time_ms*/ 250, /*workdir*/ None,
    )
    .await?;
    let process_id = opened.process_id.expect("running terminal");
    let cwd = PathUri::parse("file:///C:/workspace")?;
    {
        let mut store = manager.process_store.lock().await;
        let entry = store.processes.get_mut(&process_id).unwrap();
        entry.permissions = TerminalPermissions::for_launch(
            turn.initial_environments
                .primary()
                .expect("turn environment"),
            &turn,
            TerminalSandboxSource::Native,
            SandboxPermissions::RequireEscalated,
            /*additional_permissions*/ None,
            /*internal_permissions*/ None,
        );
        entry.environment_id = "unselected-executor".to_string();
        entry.cwd = cwd.clone();
    }
    let original = manager.resolve_terminal(process_id).await?;
    assert_eq!(original.environment_id, "unselected-executor");
    // Empty polling must complete without an approval response.
    // The test deadline must allow the minimum empty-poll wait.
    tokio::time::timeout(
        Duration::from_millis(MIN_EMPTY_YIELD_TIME_MS) + Duration::from_secs(/*secs*/ 5),
        write_stdin(&session, &turn, process_id, "", /*yield_time_ms*/ 250),
    )
    .await??;
    let input = "rejected\n";
    let denied = write_stdin(
        &session, &turn, process_id, input, /*yield_time_ms*/ 250,
    )
    .await;
    assert!(
        matches!(denied, Err(UnifiedExecError::StdinApproval(ToolError::Rejected(reason)))
        if reason.contains("select it before retrying"))
    );
    Arc::make_mut(&mut Arc::get_mut(&mut turn).unwrap().config).approvals_reviewer =
        ApprovalsReviewer::User;
    assert!(matches!(
        write_stdin(&session, &turn, process_id, input, /*yield_time_ms*/ 250).await,
        Err(UnifiedExecError::StdinApproval(ToolError::Rejected(reason)))
            if reason.contains("select it before retrying")
    ));
    manager
        .process_store
        .lock()
        .await
        .processes
        .get_mut(&process_id)
        .unwrap()
        .environment_id = turn
        .initial_environments
        .primary()
        .unwrap()
        .selection
        .environment_id
        .clone();
    for (input, decision) in [
        ("rejected\n", ReviewDecision::denied("test denial")),
        ("accepted\n", ReviewDecision::Approved),
        ("replace\n", ReviewDecision::Approved),
    ] {
        let review = async {
            loop {
                let EventMsg::ExecApprovalRequest(approval) = events.recv().await?.msg else {
                    continue;
                };
                assert_eq!(
                    (
                        approval.call_id.as_str(),
                        approval.approval_id.as_deref(),
                        approval.cwd
                    ),
                    ("call", Some("write"), cwd.clone().into())
                );
                assert!(
                    original
                        .process
                        .interaction_lock()
                        .try_lock_owned()
                        .is_err()
                );
                if input == "replace\n" {
                    let replacement = super::process_tests::remote_process(
                        WriteStatus::Accepted,
                        /*terminate_error*/ None,
                        SandboxType::None,
                    )
                    .await;
                    let mut store = manager.process_store.lock().await;
                    store.processes.get_mut(&process_id).unwrap().process = Arc::new(replacement);
                }
                session.notify_approval("write", decision).await;
                return anyhow::Ok(());
            }
        };
        let (result, reviewed) = tokio::time::timeout(Duration::from_secs(/*secs*/ 5), async {
            tokio::join!(
                write_stdin(
                    &session, &turn, process_id, input, /*yield_time_ms*/ 250
                ),
                review
            )
        })
        .await?;
        reviewed?;
        match input {
            "rejected\n" => assert!(matches!(result, Err(UnifiedExecError::StdinApproval(_)))),
            "accepted\n" => {
                let output = result?.truncated_output(DEFAULT_MAX_OUTPUT_TOKENS);
                assert!(output.contains("received:accepted"), "{output}");
                assert!(!output.contains("received:rejected"), "{output}");
            }
            _ => assert!(
                matches!(result, Err(UnifiedExecError::UnknownProcessId { process_id: id }) if id == process_id)
            ),
        }
        assert!(original.process.interaction_lock().try_lock_owned().is_ok());
    }
    // Reusing the numeric ID must also reject a subsequent write to the original terminal.
    let context = UnifiedExecContext::new(
        Arc::clone(&session),
        crate::session::step_context::StepContext::for_test(turn),
        tokio_util::sync::CancellationToken::new(),
        "stale".to_string(),
    );
    let stale = manager
        .write_stdin(
            &context,
            WriteStdinRequest {
                terminal: &original,
                input: "",
                yield_time_ms: 250,
                max_output_tokens: None,
                truncation: OutputTruncation::new(TruncationPolicy::Tokens(10_000), None),
                interaction_event: None,
                notifications: Default::default(),
            },
        )
        .await;
    assert!(
        matches!(stale, Err(UnifiedExecError::UnknownProcessId { process_id: id }) if id == process_id)
    );
    original.process.terminate();
    assert!(session.terminate_background_terminal(process_id).await);
    Ok(())
}

/// A queued write observes strict review enabled on its captured step while waiting
/// for another terminal interaction to finish.
#[tokio::test]
async fn stdin_approval_observes_strict_review_enabled_while_queued() -> anyhow::Result<()> {
    use crate::session::step_context::StepContext;
    use crate::session::tests::make_session_and_context_with_auth_and_config_and_rx;
    use crate::tools::sandboxing::ToolError;
    use codex_features::Feature;

    skip_if_sandbox!(Ok(()));
    let (session, turn, _events) = make_session_and_context_with_auth_and_config_and_rx(
        codex_login::CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        |config| {
            config.features.enable(Feature::WriteStdinApproval).unwrap();
        },
    )
    .await;
    let manager = &session.services.unified_exec_manager;
    let opened = exec_command(
        &session, &turn, "cat", /*yield_time_ms*/ 250, /*workdir*/ None,
    )
    .await?;
    let process_id = opened.process_id.expect("running terminal");
    let terminal = manager.resolve_terminal(process_id).await?;
    let context = UnifiedExecContext::new(
        Arc::clone(&session),
        StepContext::for_test(turn),
        tokio_util::sync::CancellationToken::new(),
        "write".to_string(),
    );
    let interaction = terminal.process.interaction_lock().lock_owned().await;
    let mut queued = Box::pin(manager.write_stdin(
        &context,
        WriteStdinRequest {
            terminal: &terminal,
            // NUL input must be rejected when strict review becomes required.
            input: "\0",
            yield_time_ms: 250,
            max_output_tokens: None,
            truncation: OutputTruncation::new(TruncationPolicy::Tokens(10_000), None),
            interaction_event: None,
            notifications: Default::default(),
        },
    ));
    let mut task_context = std::task::Context::from_waker(futures::task::noop_waker_ref());
    assert!(queued.as_mut().poll(&mut task_context).is_pending());
    context.step_context.turn.record_granted_permissions(
        codex_exec_server::LOCAL_ENVIRONMENT_ID,
        Default::default(),
        /*strict_auto_review*/ true,
    );
    drop(interaction);
    assert!(matches!(
        queued.await,
        Err(UnifiedExecError::StdinApproval(ToolError::Rejected(reason)))
            if reason.contains("NUL byte")
    ));
    assert!(session.terminate_background_terminal(process_id).await);
    Ok(())
}

/// Fork (background-exec-notifications): inserts a stored terminal with an exit notifier.
async fn insert_notifying_terminal(
    session: &Arc<Session>,
    turn: &TurnContext,
    process_id: i32,
    process: Arc<UnifiedExecProcess>,
    exit_notifier: Option<Arc<ExitNotifier>>,
    initial_exec_command_active: bool,
) {
    #[allow(deprecated)]
    let cwd = turn.cwd.clone();
    session
        .services
        .unified_exec_manager
        .process_store
        .lock()
        .await
        .processes
        .insert(
            process_id,
            ProcessEntry {
                process,
                plugin_metrics_sidecar: None,
                call_id: format!("call-{process_id}"),
                process_id,
                cwd: cwd.into(),
                initial_exec_command_active: Arc::new(std::sync::atomic::AtomicBool::new(
                    initial_exec_command_active,
                )),
                hook_command: "sleep 60".to_string(),
                tty: false,
                environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
                permissions: TerminalPermissions::for_launch(
                    turn.initial_environments
                        .primary()
                        .expect("turn environment"),
                    turn,
                    TerminalSandboxSource::Native,
                    SandboxPermissions::UseDefault,
                    /*additional_permissions*/ None,
                    /*internal_permissions*/ None,
                ),
                network_approval: None,
                session: Arc::downgrade(session),
                exit_notifier,
                last_used: Instant::now(),
            },
        );
}

fn exit_report(outcome: exit_notification::ExitOutcome) -> exit_notification::ExitReport {
    exit_notification::ExitReport {
        outcome,
        duration: Duration::from_secs(1),
        unread_tail: Vec::new(),
        unread_earlier_bytes: 0,
        unread_lost_bytes: 0,
        sandbox_denied: false,
    }
}

async fn live_remote_process(terminate_error: Option<&str>) -> Arc<UnifiedExecProcess> {
    Arc::new(
        super::process_tests::remote_process(
            WriteStatus::Accepted,
            terminate_error.map(str::to_string),
            SandboxType::None,
        )
        .await,
    )
}

/// Fork (R2.11): a failed client termination leaves the real outcome of the still-running
/// process to its later exit notification.
#[tokio::test]
async fn failed_terminate_keeps_the_real_exit_outcome() {
    let (session, turn) = test_session_and_turn().await;
    let manager = &session.services.unified_exec_manager;
    let process_id = manager.allocate_process_id().await;
    let notifier = ExitNotifier::new(process_id, "sleep 60".to_string(), Instant::now());
    notifier.arm();
    let process = live_remote_process(Some("exec-server unavailable")).await;
    insert_notifying_terminal(
        &session,
        &turn,
        process_id,
        process,
        Some(Arc::clone(&notifier)),
        /*initial_exec_command_active*/ false,
    )
    .await;

    assert!(!manager.terminate_process(process_id).await);

    let pending = notifier
        .on_exit(exit_report(exit_notification::ExitOutcome::Exited {
            exit_code: Some(0),
        }))
        .expect("the later exit is reported");
    assert_eq!(
        pending.report.outcome,
        exit_notification::ExitOutcome::Exited { exit_code: Some(0) }
    );
    assert!(pending.wakes());
}

/// Fork (R2.4): Codex records why it stops a terminal only while that terminal still runs.
#[tokio::test]
async fn termination_cause_marks_only_live_terminals() {
    let (session, turn) = test_session_and_turn().await;
    let live = live_remote_process(/*terminate_error*/ None).await;
    let exited = live_remote_process(/*terminate_error*/ None).await;
    exited
        .terminate_confirmed()
        .await
        .expect("exited process should terminate");
    let mut notifiers = Vec::new();
    for (process_id, process) in [(1, live), (2, exited)] {
        let notifier = ExitNotifier::new(process_id, "sleep 60".to_string(), Instant::now());
        notifier.arm();
        insert_notifying_terminal(
            &session,
            &turn,
            process_id,
            process,
            Some(Arc::clone(&notifier)),
            /*initial_exec_command_active*/ false,
        )
        .await;
        notifiers.push(notifier);
    }
    {
        let store = session
            .services
            .unified_exec_manager
            .process_store
            .lock()
            .await;
        for entry in store.processes.values() {
            entry.set_termination_cause(TerminationCause::ProcessLimit);
        }
    }
    let outcomes = notifiers
        .iter()
        .map(|notifier| {
            notifier
                .on_exit(exit_report(exit_notification::ExitOutcome::Exited {
                    exit_code: Some(143),
                }))
                .expect("reported")
                .report
                .outcome
        })
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        vec![
            exit_notification::ExitOutcome::Terminated(TerminationCause::ProcessLimit),
            exit_notification::ExitOutcome::Exited {
                exit_code: Some(143)
            },
        ]
    );
}

/// Fork (R2.12): a notification that showed everything frees exactly its own exited entry.
#[tokio::test]
async fn release_after_notification_frees_only_its_exited_terminal() {
    let (session, turn) = test_session_and_turn().await;
    let manager = &session.services.unified_exec_manager;
    let exited = live_remote_process(/*terminate_error*/ None).await;
    exited
        .terminate_confirmed()
        .await
        .expect("exited process should terminate");
    let live = live_remote_process(/*terminate_error*/ None).await;
    let owner = ExitNotifier::new(1, "true".to_string(), Instant::now());
    let other = ExitNotifier::new(1, "true".to_string(), Instant::now());
    let starting = ExitNotifier::new(2, "true".to_string(), Instant::now());
    let running = ExitNotifier::new(3, "sleep 60".to_string(), Instant::now());
    for (process_id, process, notifier, initial_exec_command_active) in [
        (1, Arc::clone(&exited), &owner, false),
        (2, Arc::clone(&exited), &starting, true),
        (3, live, &running, false),
    ] {
        insert_notifying_terminal(
            &session,
            &turn,
            process_id,
            process,
            Some(Arc::clone(notifier)),
            initial_exec_command_active,
        )
        .await;
    }
    let stored = || async {
        let mut ids = manager
            .process_store
            .lock()
            .await
            .processes
            .keys()
            .copied()
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    };

    // Another notifier with the same session ID does not own the entry.
    manager.release_after_notification(&other).await;
    manager.release_after_notification(&starting).await;
    manager.release_after_notification(&running).await;
    assert_eq!(stored().await, vec![1, 2, 3]);
    manager.release_after_notification(&owner).await;
    assert_eq!(stored().await, vec![2, 3]);
}

/// Fork (P3-3, R3.6, R4.5): a terminal stays pending until its notification is queued.
#[tokio::test]
async fn pending_terminals_include_deliveries_in_flight() {
    let (session, turn) = test_session_and_turn().await;
    let manager = &session.services.unified_exec_manager;
    let notifier = ExitNotifier::new(7, "sleep 60".to_string(), Instant::now());
    insert_notifying_terminal(
        &session,
        &turn,
        7,
        live_remote_process(/*terminate_error*/ None).await,
        Some(Arc::clone(&notifier)),
        /*initial_exec_command_active*/ false,
    )
    .await;
    assert!(!manager.has_pending_exit_notifications().await);
    notifier.arm();
    assert!(manager.has_pending_exit_notifications().await);
    let terminals = manager.pending_exit_notifications().await;
    assert_eq!(
        terminals
            .iter()
            .map(|terminal| (terminal.process_id, terminal.last_output_at))
            .collect::<Vec<_>>(),
        vec![(7, None)]
    );
    notifier
        .on_exit(exit_report(exit_notification::ExitOutcome::Exited {
            exit_code: Some(0),
        }))
        .expect("delivers");
    assert!(manager.has_pending_exit_notifications().await);
    notifier.finish_delivery();
    assert!(!manager.has_pending_exit_notifications().await);
}

/// Fork (R2.5): a shutdown owes a notice for each armed terminal it stops, nothing else.
#[tokio::test]
async fn shutdown_reports_armed_terminals_it_stops() {
    let (session, turn) = test_session_and_turn().await;
    let manager = &session.services.unified_exec_manager;
    let armed = ExitNotifier::new(11, "sleep 60".to_string(), Instant::now());
    armed.arm();
    let unarmed = ExitNotifier::new(12, "sleep 60".to_string(), Instant::now());
    for (process_id, notifier) in [(11, Some(armed)), (12, Some(unarmed)), (13, None)] {
        insert_notifying_terminal(
            &session,
            &turn,
            process_id,
            live_remote_process(/*terminate_error*/ None).await,
            notifier,
            /*initial_exec_command_active*/ false,
        )
        .await;
    }

    let terminated = manager
        .terminate_all_processes(TerminationCause::Shutdown)
        .await;
    assert_eq!(
        terminated
            .shutdown_notices
            .iter()
            .map(|notice| (notice.process_id, notice.command.as_str()))
            .collect::<Vec<_>>(),
        vec![(11, "sleep 60")]
    );
    assert!(terminated.deliveries_in_flight.is_empty());
    assert!(manager.process_store.lock().await.processes.is_empty());
}

/// Process IDs in the terminal journals under `codex_home`.
fn journaled_process_ids(codex_home: &std::path::Path) -> Vec<i64> {
    let mut ids = Vec::new();
    let dir = codex_home.join("background_terminals");
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let terminals: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
        ids.extend(
            terminals
                .iter()
                .filter_map(|terminal| terminal["process_id"].as_i64()),
        );
    }
    ids
}

/// Review of the fixes 2, N-5: with `notify_on_exit` off, only the output watch reports the
/// exit. Once the watch stops after its last notification nothing does, and the journal drops
/// the terminal, so a crash now does not make a resume report it lost.
#[tokio::test]
async fn a_watch_that_stops_itself_takes_its_terminal_out_of_the_journal() {
    let codex_home = tempfile::tempdir().expect("tempdir");
    let (session, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_config_home_and_rx(
            codex_login::CodexAuth::from_api_key("Test API Key"),
            Vec::new(),
            codex_home.path(),
            |config| {
                config
                    .features
                    .enable(codex_features::Feature::ExecExitNotifications)
                    .expect("enable exit notifications");
            },
        )
        .await;
    let notifier = ExitNotifier::new(1000, "npm run dev".to_string(), Instant::now());
    let watch = OutputWatch::with_windows(
        1000,
        "npm run dev".to_string(),
        OutputPattern::parse("tick").expect("valid pattern"),
        "turn".to_string(),
        OutputWatchWindows::default(),
        notifier.exit_preference(),
    );
    notifier.set_output_watch(Some(Arc::clone(&watch)));
    insert_notifying_terminal(
        &session,
        &turn,
        1000,
        live_remote_process(/*terminate_error*/ None).await,
        Some(Arc::clone(&notifier)),
        /*initial_exec_command_active*/ false,
    )
    .await;
    assert!(notifier.arm().is_none());
    notifier.disarm_exit();
    session.fork_sync_terminal_journal().await;
    assert_eq!(journaled_process_ids(codex_home.path()), vec![1000]);

    // Nine notifications in the past, then the last one through the session.
    let mut now = Instant::now()
        .checked_sub(Duration::from_secs(120))
        .expect("a monotonic clock that runs for two minutes");
    for _ in 1..OUTPUT_WATCH_MAX_NOTIFICATIONS {
        watch.observe_chunk(b"tick\n");
        watch.match_now(/*final_batch*/ false, now);
        watch
            .take_for_queue(
                /*queued*/ false,
                now + OUTPUT_WATCH_BATCH_WINDOW,
                /*exited*/ false,
                &codex_utils_string::untrusted::UntrustedBoundary::random(),
            )
            .expect("a due notification");
        now += OUTPUT_WATCH_MIN_INTERVAL + OUTPUT_WATCH_BATCH_WINDOW;
    }
    session.fork_sync_terminal_journal().await;
    assert_eq!(
        journaled_process_ids(codex_home.path()),
        vec![1000],
        "the watch still reports the exit"
    );
    watch.observe_chunk(b"tick\n");
    watch.match_now(/*final_batch*/ false, now);
    assert!(
        session
            .deliver_output_watch_notification(&watch, /*exited*/ false)
            .await
    );
    assert!(watch.is_stopped());
    assert_eq!(journaled_process_ids(codex_home.path()), Vec::<i64>::new());
}
