//! Generates the Code Mode cell that runs a workflow script (UC2, R2.3, R2.4, R2.6, R2.7, R2.8)
//! and the function a nested `workflow(name)` runs in that cell (UC12, R12.1).
//!
//! The prelude occupies the first line in front of the script, so every line of the script
//! keeps its number inside the cell and V8 errors point at the author's lines.

/// Host tool that runs one `agent()` call.
pub const TOOL_AGENT: &str = "__wf_agent";
/// Host tool that records `phase`, `log`, the result and script errors.
pub const TOOL_EVENT: &str = "__wf_event";
/// Host tool that reports budget usage.
pub const TOOL_BUDGET: &str = "__wf_budget";
/// Host tool that loads a saved script for `workflow(name)` and records its outcome.
pub const TOOL_WORKFLOW: &str = "__wf_workflow";
/// Host tools a workflow cell may call.
pub const HOST_TOOLS: &[&str] = &[TOOL_AGENT, TOOL_EVENT, TOOL_BUDGET, TOOL_WORKFLOW];

/// Most thunks or items accepted by `parallel` and `pipeline`.
pub const MAX_FAN_OUT: usize = 4096;
/// Largest accepted run result, in bytes of JSON.
pub const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
/// Module name V8 uses in stack traces of the cell.
pub const CELL_MODULE_NAME: &str = "exec_main.mjs";
/// Name shown to the model instead of [`CELL_MODULE_NAME`].
pub const SCRIPT_DISPLAY_NAME: &str = "workflow.js";

// Every statement ends with `;` and there are no line comments: the prelude is joined into
// one line.
const PRELUDE: &str = r#"
const { agent, parallel, pipeline, phase, log, workflow, budget, args, WorkflowDeterminismError, WorkflowSchemaError, WorkflowNestingError, __wf_finish, __wf_fail } = (() => {
  const host = globalThis.tools;
  const indirectEval = globalThis.eval;
  for (const name of ["tools", "EXEC_TOOLS", "notify", "yield_control", "store", "load", "setTimeout", "clearTimeout", "exit", "text", "image", "audio", "generatedImage"]) {
    try { delete globalThis[name]; } catch (_) {}
  }
  class WorkflowDeterminismError extends Error {
    constructor(api) {
      super(`${api} is not allowed in workflow scripts: they must be deterministic; pass times or random values in args`);
      this.name = "WorkflowDeterminismError";
    }
  }
  class WorkflowSchemaError extends Error {
    constructor(message) { super(message); this.name = "WorkflowSchemaError"; }
  }
  class WorkflowNestingError extends Error {
    constructor(message) { super(message); this.name = "WorkflowNestingError"; }
  }
  const forbid = (api) => () => { throw new WorkflowDeterminismError(api); };
  const NativeDate = Date;
  function WorkflowDate(...values) {
    if (!new.target) { throw new WorkflowDeterminismError("Date()"); }
    if (values.length === 0) { throw new WorkflowDeterminismError("new Date()"); }
    return Reflect.construct(NativeDate, values, new.target);
  }
  Object.setPrototypeOf(WorkflowDate, NativeDate);
  WorkflowDate.prototype = NativeDate.prototype;
  WorkflowDate.now = forbid("Date.now()");
  globalThis.Date = WorkflowDate;
  Math.random = forbid("Math.random()");
  if (typeof globalThis.performance === "object" && globalThis.performance !== null) {
    try { globalThis.performance.now = forbid("performance.now()"); } catch (_) {}
  }
  const quiet = (promise) => { Promise.resolve(promise).catch(() => {}); };
  const describe = (error) => String((error && error.stack) || error);
  const render = (value) => {
    if (typeof value === "string") { return value; }
    try { const json = JSON.stringify(value); return json === undefined ? String(value) : json; } catch (_) { return String(value); }
  };
  let eventSeq = 0;
  let callSeq = 0;
  let nestedSeq = 0;
  const event = (payload) => quiet(host.__wf_event({ ...payload, seq: ++eventSeq }));
  const makeScope = (phasePrefix, logPrefix) => {
    let currentPhase = null;
    const phase = (title) => { currentPhase = phasePrefix + String(title); event({ kind: "phase", title: currentPhase }); };
    const log = (...parts) => { event({ kind: "log", message: logPrefix + parts.map(render).join(" ") }); };
    const agent = async (prompt, opts) => {
      if (opts !== undefined && (opts === null || typeof opts !== "object" || Array.isArray(opts))) {
        throw new TypeError("agent(prompt, opts): opts must be an object");
      }
      const seq = ++callSeq;
      const outcome = await host.__wf_agent({ seq, prompt: String(prompt), opts: { ...(opts || {}) }, phase: currentPhase });
      if (outcome && outcome.error === "schema") { throw new WorkflowSchemaError(outcome.message); }
      if (outcome && outcome.error === "type") { throw new TypeError(outcome.message); }
      return outcome && outcome.ok ? outcome.value : null;
    };
    const console = Object.freeze({ log, info: log, warn: log, error: log, debug: log });
    return { phase, log, agent, console };
  };
  const root = makeScope("", "");
  globalThis.console = root.console;
  const settle = (what, index, run) => {
    let promise;
    try { promise = Promise.resolve(run()); } catch (error) { promise = Promise.reject(error); }
    return promise.catch((error) => { event({ kind: "log", message: `${what} item ${index} failed: ${describe(error)}` }); return null; });
  };
  async function parallel(thunks) {
    if (!Array.isArray(thunks)) { throw new TypeError("parallel(thunks) expects an array of functions"); }
    if (thunks.length > __MAX_FAN_OUT__) { throw new RangeError(`parallel() accepts at most __MAX_FAN_OUT__ thunks, got ${thunks.length}`); }
    thunks.forEach((thunk, index) => {
      if (typeof thunk !== "function") { throw new TypeError(`parallel(): item ${index} is not a function; wrap each call as () => agent(...)`); }
    });
    return Promise.all(thunks.map((thunk, index) => settle("parallel", index, thunk)));
  }
  async function pipeline(items, ...stages) {
    if (!Array.isArray(items)) { throw new TypeError("pipeline(items, ...stages) expects an array of items"); }
    if (items.length > __MAX_FAN_OUT__) { throw new RangeError(`pipeline() accepts at most __MAX_FAN_OUT__ items, got ${items.length}`); }
    stages.forEach((stage, index) => {
      if (typeof stage !== "function") { throw new TypeError(`pipeline(): stage ${index} is not a function`); }
    });
    return Promise.all(items.map(async (item, index) => {
      let value = item;
      for (const [stageIndex, stage] of stages.entries()) {
        try { value = await stage(value, index); } catch (error) {
          event({ kind: "log", message: `pipeline item ${index} stage ${stageIndex} failed: ${describe(error)}` });
          return null;
        }
        if (value === null || value === undefined) { return null; }
      }
      return value;
    }));
  }
  const budget = Object.freeze({
    limit: __BUDGET_LIMIT__,
    used: async () => (await host.__wf_budget({})).used,
    remaining: async () => (await host.__wf_budget({})).remaining,
  });
  const deepFreeze = (value) => {
    if (value !== null && typeof value === "object" && !Object.isFrozen(value)) {
      Object.freeze(value);
      for (const key of Object.keys(value)) { deepFreeze(value[key]); }
    }
    return value;
  };
  const nestedWorkflow = async (name) => {
    throw new WorkflowNestingError(`workflow(${JSON.stringify(String(name))}): a nested workflow cannot start another workflow`);
  };
  async function workflow(name, nestedArgs) {
    const call = ++nestedSeq;
    let argsJson;
    try { argsJson = JSON.stringify(nestedArgs === undefined ? null : nestedArgs); } catch (error) {
      throw new TypeError(`workflow(name, args): args must be JSON: ${describe(error)}`);
    }
    const loaded = await host.__wf_workflow({ kind: "load", call, name: String(name), args_json: argsJson === undefined ? "null" : argsJson });
    if (!loaded || !loaded.ok) {
      throw new Error((loaded && loaded.message) || `workflow(${JSON.stringify(String(name))}) could not be loaded`);
    }
    const scope = makeScope(`${loaded.name}/`, `[${loaded.name}] `);
    let value = null;
    let ok = true;
    let error = null;
    try {
      const body = indirectEval(loaded.source);
      const result = await body(scope.agent, parallel, pipeline, scope.phase, scope.log, nestedWorkflow, budget, deepFreeze(JSON.parse(loaded.args_json)), scope.console);
      const json = JSON.stringify(result === undefined ? null : result);
      value = json === undefined ? null : JSON.parse(json);
    } catch (caught) {
      ok = false;
      error = describe(caught);
      value = null;
    }
    await host.__wf_workflow({ kind: "finished", call, name: loaded.name, ok, error });
    return value;
  }
  const args = deepFreeze(JSON.parse(__ARGS_JSON__));
  const __wf_finish = async (value) => {
    let json;
    try { json = JSON.stringify(value === undefined ? null : value); } catch (error) {
      await host.__wf_event({ kind: "error", seq: ++eventSeq, message: `the workflow result is not JSON-serializable: ${describe(error)}` });
      return;
    }
    await host.__wf_event({ kind: "result", seq: ++eventSeq, value_json: json === undefined ? "null" : json });
  };
  const __wf_fail = async (error) => { await host.__wf_event({ kind: "error", seq: ++eventSeq, message: describe(error) }); };
  return { agent: root.agent, parallel, pipeline, phase: root.phase, log: root.log, workflow, budget, args, WorkflowDeterminismError, WorkflowSchemaError, WorkflowNestingError, __wf_finish, __wf_fail };
})();
"#;

/// UC2: build the cell source for a validated script body: the prelude on the first line,
/// the body inside `async function __wf_main()`, and a trailer that reports the result or
/// the error through the event tool.
pub fn build_cell_source(body: &str, args_json: &str, budget_limit: Option<u64>) -> String {
    let args_literal = serde_json::to_string(args_json).unwrap_or_else(|_| "\"null\"".to_string());
    let budget_literal = budget_limit.map_or_else(|| "null".to_string(), |limit| limit.to_string());
    let prelude = PRELUDE
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .replace("__MAX_FAN_OUT__", &MAX_FAN_OUT.to_string())
        .replace("__BUDGET_LIMIT__", &budget_literal)
        .replace("__ARGS_JSON__", &args_literal);
    let mut source = String::with_capacity(prelude.len() + body.len() + 128);
    source.push_str(&prelude);
    source.push_str(" async function __wf_main() {");
    source.push_str(body);
    source.push_str("\n}\nawait __wf_main().then(__wf_finish, __wf_fail);\n");
    source
}

/// UC12, R12.1: the source of the function a nested `workflow(name)` runs: an async function
/// of the workflow globals whose body is the validated saved script. The body starts on the
/// first line, so V8 locations under `sourceURL=<name>.js` match the saved script's lines.
/// `name` is validated by the `meta.name` rule.
pub fn build_nested_source(body: &str, name: &str) -> String {
    format!(
        "(async function (agent, parallel, pipeline, phase, log, workflow, budget, args, console) {{\"use strict\";{body}\n}})\n//# sourceURL={name}.js\n"
    )
}

/// Rewrites V8 locations in `text` to name the author's script instead of the cell.
pub fn display_error(text: &str) -> String {
    text.replace(CELL_MODULE_NAME, SCRIPT_DISPLAY_NAME)
}

#[cfg(test)]
#[path = "prelude_tests.rs"]
mod tests;
