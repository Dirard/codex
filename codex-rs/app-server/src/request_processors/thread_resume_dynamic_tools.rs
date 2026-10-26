//! Fork: `thread/resume.dynamicTools` replaces the dynamic tools of the resumed thread
//! (`docs/fork/04-app-server-protocol.md`, section 6). A child module of
//! `thread_processor`, which re-exports these functions.

use super::*;

/// Validates replacement tools before a loaded thread can be rebuilt for them.
pub(super) fn fork_validate_resume_dynamic_tools(
    params: &ThreadResumeParams,
) -> Result<(), JSONRPCErrorError> {
    if let Some(dynamic_tools) = params.dynamic_tools.as_deref()
        && !dynamic_tools.is_empty()
    {
        validate_dynamic_tools(dynamic_tools).map_err(invalid_request)?;
    }
    Ok(())
}

/// Replacement dynamic tools follow the same rebuild-or-ignore policy as the other resume
/// overrides of a running thread.
pub(super) async fn fork_dynamic_tools_mismatch(
    params: &ThreadResumeParams,
    existing_thread: &codex_core::CodexThread,
) -> Option<String> {
    if params
        .dynamic_tools
        .as_ref()
        .is_some_and(|tools| !tools.is_empty())
    {
        dynamic_tools_override_mismatch(
            params.dynamic_tools.as_deref(),
            &existing_thread.dynamic_tools().await,
        )
    } else {
        None
    }
}

/// A non-empty `thread/resume.dynamicTools` list that differs from the loaded thread's
/// tools is a resume override; omitted or empty keeps the active tools.
pub(super) fn dynamic_tools_override_mismatch(
    requested: Option<&[DynamicToolSpec]>,
    active: &[DynamicToolSpec],
) -> Option<String> {
    let requested = requested.filter(|tools| !tools.is_empty())?;
    let names = |tools: &[DynamicToolSpec]| {
        tools
            .iter()
            .map(|tool| match tool {
                DynamicToolSpec::Function(function) => function.name.as_str(),
                DynamicToolSpec::Namespace(namespace) => namespace.name.as_str(),
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    (requested != active).then(|| {
        format!(
            "dynamic_tools requested=[{}] active=[{}]",
            names(requested),
            names(active)
        )
    })
}
