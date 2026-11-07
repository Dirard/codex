// Built-in workflow `review` (specs/builtin-workflows-2026-11-07.md, UC5): a multi-agent code
// review. One explorer resolves the target, finders look at groups of files through several
// lenses, independent skeptics try to refute every candidate, critics ask what was missed, and
// the script itself builds the report, so the same answers always give the same result.
// Nothing is dropped silently: every cut is logged and counted in `omitted` or `coverage`.
export const meta = {
  name: "review",
  description: "Multi-agent code review: scope the changes, find bugs through several lenses, keep only findings that independent skeptics fail to refute, report them with coverage",
  phases: ["Scope", "Find", "Verify", "Critic", "Report"],
  args: {
    type: "object",
    additionalProperties: false,
    properties: {
      effort: { type: "string", enum: ["low", "medium", "high", "xhigh", "max"], description: "breadth and verification depth; default \"medium\"" },
      target: { type: "string", description: "what to review in words: a branch, commit, range, paths or a request with a focus; default: uncommitted changes" },
      base: { type: "string", description: "changes since the merge base of HEAD with this branch, working tree included" },
      commit: { type: "string", description: "the changes of this commit" },
      uncommitted: { type: "boolean", description: "staged, unstaged and untracked changes" },
      paths: { type: "array", items: { type: "string" }, description: "review only these paths" },
      focus: { type: "string", description: "extra instruction for every reviewer" },
      max_findings: { anyOf: [{ type: "integer", minimum: 1 }, { const: "all" }], description: "report cap; default by effort" },
    },
  },
};

// Every schema an agent answers in (R1.9).
const SCHEMAS = {
  scope: {
    type: "object",
    additionalProperties: false,
    required: ["target", "diff_command", "revision", "files", "focus", "empty_reason"],
    properties: {
      target: { type: "string" },
      diff_command: { type: "string" },
      revision: { type: ["string", "null"] },
      files: {
        type: "array",
        items: {
          type: "object",
          additionalProperties: false,
          required: ["path", "status", "added", "removed"],
          properties: {
            path: { type: "string" },
            status: { type: "string", enum: ["added", "modified", "deleted", "renamed", "untracked"] },
            added: { type: "integer" },
            removed: { type: "integer" },
          },
        },
      },
      focus: { type: ["string", "null"] },
      empty_reason: { type: ["string", "null"] },
    },
  },
  findings: {
    type: "object",
    additionalProperties: false,
    required: ["findings", "notes"],
    properties: {
      findings: {
        type: "array",
        items: {
          type: "object",
          additionalProperties: false,
          required: ["file", "line", "end_line", "severity", "title", "failure_scenario", "evidence", "confidence"],
          properties: {
            file: { type: "string" },
            line: { type: "integer" },
            end_line: { type: ["integer", "null"] },
            severity: { type: "string", enum: ["P0", "P1", "P2", "P3"] },
            title: { type: "string" },
            failure_scenario: { type: "string" },
            evidence: { type: "string" },
            confidence: { type: "number" },
          },
        },
      },
      notes: { type: ["string", "null"] },
    },
  },
  verdict: {
    type: "object",
    additionalProperties: false,
    required: ["verdict", "reason", "severity", "line"],
    properties: {
      verdict: { type: "string", enum: ["upheld", "refuted", "uncertain"] },
      reason: { type: "string" },
      severity: { anyOf: [{ type: "string", enum: ["P0", "P1", "P2", "P3"] }, { type: "null" }] },
      line: { type: ["integer", "null"] },
    },
  },
};

// Upper bounds per effort before the plan is fitted to the run's limits (UC5 effort table).
const EFFORT = {
  low: { lenses: ["correctness"], filesPerFinder: 12, findersPerRound: 3, rounds: 1, verifyCap: 6, skeptics: ["reproduce"], critics: 0, perFinder: 3, verdicts: ["confirmed"], severities: ["P0", "P1"], maxFindings: 5 },
  medium: { lenses: ["correctness", "robustness"], filesPerFinder: 8, findersPerRound: 6, rounds: 1, verifyCap: 12, skeptics: ["reproduce", "reachability"], critics: 0, perFinder: 5, verdicts: ["confirmed"], severities: ["P0", "P1", "P2"], maxFindings: 10 },
  high: { lenses: ["correctness", "robustness", "concurrency", "security"], filesPerFinder: 4, findersPerRound: 12, rounds: 1, verifyCap: 20, skeptics: ["reproduce", "reachability", "impact"], critics: 1, perFinder: 8, verdicts: ["confirmed", "contested"], severities: ["P0", "P1", "P2"], maxFindings: 20 },
  xhigh: { lenses: ["correctness", "robustness", "concurrency", "security", "integration"], filesPerFinder: 2, findersPerRound: 20, rounds: 2, verifyCap: 30, skeptics: ["reproduce", "reachability", "impact"], critics: 1, perFinder: 10, verdicts: ["confirmed", "contested"], severities: ["P0", "P1", "P2", "P3"], maxFindings: 30 },
  max: { lenses: ["correctness", "robustness", "concurrency", "security", "integration"], filesPerFinder: 1, findersPerRound: 40, rounds: 3, verifyCap: 50, skeptics: ["reproduce", "reproduce", "reachability", "reachability", "impact"], critics: 2, perFinder: 12, verdicts: ["confirmed", "contested"], severities: ["P0", "P1", "P2", "P3"], maxFindings: "all" },
};

const LENSES = {
  correctness: "wrong logic: conditions, boundaries and off-by-one errors, wrong API use, broken invariants, wrong results",
  robustness: "error handling and edge values: empty input, null, overflow, Unicode, resource leaks, panics, unwrap and unchecked assumptions",
  concurrency: "races, deadlocks, ordering of events, async cancellation, shared mutable state",
  security: "injection, path traversal, secrets, unsafe deserialization, missing permission checks",
  integration: "callers and dependents outside the diff that the change breaks: contracts, on-disk and protocol formats, compatibility",
};

const SKEPTICS = {
  reproduce: "trace the code path as written, step by step, and check whether the described failure scenario actually happens",
  reachability: "check whether real callers and inputs can reach this path, and whether a guard elsewhere (validation, types, an earlier check) prevents it",
  impact: "check whether the consequences are real and the severity is right",
};

const CRITIC_TASKS = [
  "Name defects in failure modes and changed areas the reviewers did not examine: lenses that were not used, edge cases, error paths and changed functions no finding covers.",
  "Name defects at callers and contracts outside the diff that the reviewers did not examine: code that uses what changed, persisted or wire formats, configuration and compatibility.",
];

const SEVERITIES = ["P0", "P1", "P2", "P3"];
const LENS_ORDER = ["correctness", "robustness", "concurrency", "security", "integration", "critic"];
// Planning estimate of one focused explorer, in charged tokens (R5.5.2, R5.5.3).
const EST_AGENT_TOKENS = 25000;
const MAX_UNVERIFIED_TITLES = 20;
const MAX_UNREVIEWED_LISTED = 50;

const READ_ONLY = "Rules: this is read-only work. Do not create, modify or delete files and do not run commands that change the repository or its state; read files and run read-only commands only. Your final message must be only the JSON object of the required schema.";

// The one agent() call: a read-only explorer whose task starts with its role tag (R1.10, R1.11).
const ask = (role, label, schema, task) =>
  agent(`[codex-review:${role}]\n${task}\n\n${READ_ONLY}`, { schema, label, agent_type: "explorer" });

const compare = (left, right) => (left < right ? -1 : left > right ? 1 : 0);
const rankOf = (severity) => {
  const index = SEVERITIES.indexOf(severity);
  return index < 0 ? SEVERITIES.length : index;
};
const clip = (text, limit) => {
  const value = String(text ?? "");
  return value.length <= limit ? value : `${value.slice(0, limit - 1)}…`;
};
const plural = (count, word) => `${count} ${word}${count === 1 ? "" : "s"}`;
const chunk = (items, size) => {
  const groups = [];
  for (let start = 0; start < items.length; start += size) {
    groups.push(items.slice(start, start + size));
  }
  return groups;
};
const where = (candidate) => `${candidate.file}:${candidate.line}`;
const bySeverityAndConfidence = (left, right) =>
  rankOf(left.severity) - rankOf(right.severity) ||
  right.confidence - left.confidence ||
  compare(left.file, right.file) ||
  left.line - right.line;

const a = args ?? {};
const level = a.effort ?? "medium";
const row = EFFORT[level];
const notes = [];
const note = (line) => {
  notes.push(line);
  log(line);
};

// R5.1.1: at most one explicit target, checked before any agent starts.
const explicit = [
  a.uncommitted === true ? "uncommitted" : null,
  a.base !== undefined ? "base" : null,
  a.commit !== undefined ? "commit" : null,
].filter((name) => name !== null);
if (explicit.length > 1) {
  throw new Error(`review: pass at most one of uncommitted, base and commit (got ${explicit.join(", ")}); nothing was reviewed`);
}
if (!engine.read_only_explorers) {
  note("explorer_profile is off: reviewers are not sandboxed read-only; they were told not to modify files");
}

// ---------------------------------------------------------------------------------------
// Scope (UC5.1)
// ---------------------------------------------------------------------------------------

const scopeTask = () => {
  let target;
  if (a.commit !== undefined) {
    target = `The changes of commit ${JSON.stringify(a.commit)}. Resolve it with git rev-parse. diff_command: "git diff <parent> <commit> --" with full SHAs (for a root commit "git show --format= <commit> --"); revision: the commit's full SHA.`;
  } else if (a.base !== undefined) {
    target = `The changes since the merge base of HEAD with ${JSON.stringify(a.base)}, working tree included: find it with "git merge-base HEAD ${a.base}". diff_command: "git diff <merge-base SHA> --"; revision: null. Untracked files count as added.`;
  } else if (a.uncommitted === true || a.target === undefined) {
    target = `Uncommitted changes: staged, unstaged and untracked files against HEAD. diff_command: "git diff HEAD --"; revision: null. Untracked files get status "untracked" and count all their lines as added.`;
  } else {
    target = `The user's words: ${JSON.stringify(a.target)}. They may name a branch (review its changes since its merge base with the current branch), a commit, a range, paths (review their uncommitted changes, or the whole files when they have none) or a focus such as "security of the auth flow"; resolve them to one set of changes.`;
  }
  const extra = explicit.length > 0 && a.target !== undefined
    ? `\nThe user also said: ${JSON.stringify(a.target)}; take a focus from it.`
    : "";
  return `You resolve what a code review covers, using read-only git commands (git status, git diff --numstat, git log, git show, git rev-parse, git merge-base) in the current repository.
Target: ${target}${extra}
Answer:
- target: a short description of what is reviewed, such as "uncommitted changes on main" or "commit 1a2b3c4 (Fix the parser)".
- diff_command: a read-only command that prints the unified diff of the target for the paths appended after it.
- revision: the revision whose file versions reviewers read with "git show <revision>:<path>", or null to read the working tree.
- files: every changed file with its status (added, modified, deleted, renamed, untracked) and its added and removed line counts, paths relative to the repository root.
- focus: a review focus the user's words ask for, or null.
- empty_reason: null when files is not empty; otherwise why there is nothing to review, such as "no uncommitted changes".`;
};

phase("Scope");
const scope = await ask("scope", "scope", SCHEMAS.scope, scopeTask());
if (scope === null) {
  throw new Error("review: the target could not be resolved (the scope agent gave no answer); nothing was reviewed");
}

// R5.1.4: changed files, filtered by `paths` and sorted by the size of their changes.
const inPaths = (path) =>
  a.paths === undefined ||
  a.paths.some((prefix) => {
    const trimmed = prefix.replace(/\/+$/, "");
    return path === trimmed || path.startsWith(`${trimmed}/`);
  });
const seen = new Set();
const changed = [];
for (const file of scope.files) {
  const path = file.path.trim();
  if (path === "" || seen.has(path) || !inPaths(path)) {
    continue;
  }
  seen.add(path);
  changed.push({ path, status: file.status, added: Math.max(0, file.added), removed: Math.max(0, file.removed) });
}
changed.sort((left, right) => right.added + right.removed - (left.added + left.removed) || compare(left.path, right.path));
const reviewable = changed.filter((file) => file.status !== "deleted");
const deletedCount = changed.length - reviewable.length;
if (deletedCount > 0) {
  note(`${plural(deletedCount, "deleted file")} not given to finders: no code to read`);
}
const focus = [a.focus, scope.focus].filter((text) => typeof text === "string" && text.trim() !== "").join("; ");

const emptyOmitted = () => ({ refuted: 0, contested: 0, below_severity: 0, over_cap: 0, unverified: 0, unverified_titles: [] });
if (reviewable.length === 0) {
  // R5.1.3: nothing to review completes the run without findings.
  const reason = scope.empty_reason ??
    (changed.length > 0 ? "only deleted files changed" : a.paths !== undefined ? "no changed file matches paths" : "no changes");
  log(`nothing to review: ${reason}`);
  phase("Report");
  return {
    workflow: "review",
    version: 1,
    target: scope.target,
    effort: level,
    summary: `0 findings: nothing to review (${reason})`,
    findings: [],
    omitted: emptyOmitted(),
    coverage: { files_total: changed.length, files_reviewed: 0, unreviewed: [], unreviewed_total: 0, lenses: [], rounds: 0, critics: 0, notes },
  };
}

// ---------------------------------------------------------------------------------------
// Plan (UC5.5): the effort row fitted to the finders per round, the agent limit and budget.
// ---------------------------------------------------------------------------------------

const plan = {
  lenses: [...row.lenses],
  filesPerFinder: row.filesPerFinder,
  groupCount: chunk(reviewable, row.filesPerFinder).length,
  rounds: row.rounds,
  verifyCap: row.verifyCap,
  skeptics: [...row.skeptics],
  critics: row.critics,
  reasons: [],
};
const pairCount = () => plan.lenses.length * plan.groupCount;
const worstCase = () => 1 + pairCount() * plan.rounds + plan.critics + plan.verifyCap * plan.skeptics.length;
const change = (what, from, to, reason) => {
  const line = `plan: ${what} ${from} → ${to} to fit ${reason}`;
  log(line);
  notes.push(line);
  if (!plan.reasons.includes(reason)) {
    plan.reasons.push(reason);
  }
};
// The reduction ladder of R5.5.1, in order; each step only while the plan does not fit.
const STEPS = {
  widen: (fits, reason) => {
    const widest = row.filesPerFinder * 2;
    const groups = Math.ceil(reviewable.length / widest);
    if (fits() || plan.filesPerFinder >= widest || groups >= plan.groupCount) {
      return;
    }
    change("files per finder", plan.filesPerFinder, `${widest} (${plural(groups, "group")})`, reason);
    plan.filesPerFinder = widest;
    plan.groupCount = groups;
  },
  lenses: (fits, reason) => {
    const before = plan.lenses.length;
    const dropped = [];
    while (!fits() && plan.lenses.length > 1) {
      dropped.unshift(plan.lenses.pop());
    }
    if (dropped.length > 0) {
      change("lenses", before, `${plan.lenses.length} (left out ${dropped.join(", ")})`, reason);
    }
  },
  votes: (fits, reason) => {
    const before = plan.skeptics.length;
    while (!fits() && plan.skeptics.length > 1) {
      plan.skeptics.pop();
    }
    if (plan.skeptics.length < before) {
      change("votes per candidate", before, plan.skeptics.length, reason);
    }
  },
  rounds: (fits, reason) => {
    const before = plan.rounds;
    while (!fits() && plan.rounds > 1) {
      plan.rounds -= 1;
    }
    if (plan.rounds < before) {
      change("find rounds", before, plan.rounds, reason);
    }
  },
  critics: (fits, reason) => {
    const before = plan.critics;
    while (!fits() && plan.critics > 0) {
      plan.critics -= 1;
    }
    if (plan.critics < before) {
      change("critics", before, plan.critics, reason);
    }
  },
  verifyCap: (fits, reason, cap) => {
    if (fits()) {
      return;
    }
    const left = cap - 1 - pairCount() * plan.rounds - plan.critics;
    const target = Math.max(1, Math.floor(left / plan.skeptics.length));
    if (target < plan.verifyCap) {
      change("verification cap", plan.verifyCap, target, reason);
      plan.verifyCap = target;
    }
  },
  groups: (fits, reason) => {
    const before = plan.groupCount;
    while (!fits() && plan.groupCount > 1) {
      plan.groupCount -= 1;
    }
    if (plan.groupCount < before) {
      const left = reviewable.length - Math.min(reviewable.length, plan.groupCount * plan.filesPerFinder);
      change("file groups", before, `${plan.groupCount} (${plural(left, "file")} with the smallest changes unreviewed)`, reason);
    }
  },
};
const LADDER = ["widen", "lenses", "votes", "rounds", "critics", "verifyCap", "groups"];
const fitTo = (cap, reason) => {
  const fits = () => worstCase() <= cap;
  for (const step of LADDER) {
    STEPS[step](fits, reason, cap);
  }
  if (!fits()) {
    note(`plan: still ${worstCase()} agents at most for ${reason}; later calls may resolve to null`);
  }
};

// The finders of one round stay within the effort's bound.
const finderBound = () => pairCount() <= row.findersPerRound;
STEPS.widen(finderBound, `${row.findersPerRound} finders per round at effort ${level}`);
STEPS.groups(finderBound, `${row.findersPerRound} finders per round at effort ${level}`);
// R5.5.1: the worst case fits the run's agent limit.
fitTo(engine.max_agents_per_run, `max_agents_per_run=${engine.max_agents_per_run}`);
// R5.5.2: and the budget, at the planning estimate per agent.
if (budget.limit !== null) {
  fitTo(Math.floor(budget.limit / EST_AGENT_TOKENS), `budget_tokens=${budget.limit} at ~${EST_AGENT_TOKENS} tokens per agent`);
}

const groups = chunk(reviewable, plan.filesPerFinder).slice(0, plan.groupCount);
const planned = groups.flat();
const reviewedPaths = new Set();
const perFinder = row.perFinder;

// R5.5.3: the part of `count` agents (`unit` agents each) the remaining budget covers.
const affordable = async (count, unit, what) => {
  const remaining = await budget.remaining();
  if (remaining === null || remaining >= count * unit * EST_AGENT_TOKENS) {
    return count;
  }
  const covered = Math.max(0, Math.floor(remaining / (unit * EST_AGENT_TOKENS)));
  const line = `${what}: the remaining budget of ${remaining} tokens covers ${covered} of ${count}`;
  log(line);
  notes.push(line);
  return covered;
};

// ---------------------------------------------------------------------------------------
// Find (UC5.2)
// ---------------------------------------------------------------------------------------

const revisionLine = () =>
  scope.revision === null
    ? "Read files from the working tree."
    : `Read files at the target's version with "git show ${scope.revision}:<path>".`;
const fileLine = (file) => `- ${file.path} (${file.status}, +${file.added}/-${file.removed})`;
const SEVERITY_SCALE = "Severity: P0 blocks the release (data loss, security hole, crash on a main path); P1 is urgent (common wrong behavior); P2 is normal (wrong behavior in less common cases); P3 is minor (rare and low impact).";

const findTask = (pair, known) => {
  const paths = new Set(pair.files.map((file) => file.path));
  const already = known.filter((candidate) => paths.has(candidate.file));
  return `You are one reviewer of a multi-agent code review. Review the files below through one lens and report real defects only.
Target: ${scope.target}
Diff of a file: run "${scope.diff_command} <path>". ${revisionLine()}
Files (status, +added/-removed lines):
${pair.files.map(fileLine).join("\n")}
Lens ${pair.lens}: ${LENSES[pair.lens]}.${focus === "" ? "" : `\nFocus of this review: ${focus}`}${already.length === 0 ? "" : `\nAlready found, do not report again; look for other defects:\n${already.map((candidate) => `- ${where(candidate)} ${candidate.title}`).join("\n")}`}
Read the changes and enough surrounding code (callers, callees, tests) to be sure. Report at most ${perFinder} findings, most severe first. Report only defects with a concrete failure scenario: a specific input or sequence of events and the wrong outcome it causes. No style, naming, refactoring or "could be improved" remarks, nothing you did not tie to code you read.
${SEVERITY_SCALE}
Fields: file is the path from the repository root; line is the 1-based line in the target's version of the file, end_line the last line or null; evidence cites the code that shows the defect in at most 300 characters; confidence is 0 to 1. With no defects answer {"findings": [], "notes": null}; notes may say what you could not check.`;
};

const candidates = [];
const words = (title) => new Set(String(title).toLowerCase().split(/[^a-z0-9]+/).filter((word) => word.length >= 3));
const similarTitles = (left, right) => {
  const one = words(left);
  const two = words(right);
  let shared = 0;
  for (const word of one) {
    if (two.has(word)) {
      shared += 1;
    }
  }
  const union = one.size + two.size - shared;
  return union > 0 && shared / union >= 0.5;
};
const lensRank = (lens) => {
  const index = LENS_ORDER.indexOf(lens);
  return index < 0 ? LENS_ORDER.length : index;
};
// R5.2.3: merge a finding into the candidates; true when it is new.
const mergeFinding = (finding, lens) => {
  const same = candidates.find((candidate) =>
    candidate.file === finding.file &&
    Math.abs(candidate.line - finding.line) <= 3 &&
    (candidate.lenses.includes(lens) || similarTitles(candidate.title, finding.title)));
  if (same === undefined) {
    candidates.push({ ...finding, lenses: [lens], id: candidates.length + 1 });
    return true;
  }
  if (rankOf(finding.severity) < rankOf(same.severity)) {
    same.severity = finding.severity;
  }
  same.confidence = Math.max(same.confidence, finding.confidence);
  if (finding.failure_scenario.length > same.failure_scenario.length) {
    same.failure_scenario = finding.failure_scenario;
  }
  if (!same.lenses.includes(lens)) {
    same.lenses.push(lens);
    same.lenses.sort((left, right) => lensRank(left) - lensRank(right));
  }
  return false;
};
const normalize = (finding) => {
  const file = String(finding.file).trim().replace(/^\.\//, "");
  const title = String(finding.title).trim();
  if (file === "" || title === "") {
    return null;
  }
  const line = Math.max(1, Math.trunc(finding.line));
  const endLine = finding.end_line === null ? null : Math.max(line, Math.trunc(finding.end_line));
  return {
    file,
    line,
    end_line: endLine,
    severity: finding.severity,
    title: clip(title, 120),
    failure_scenario: String(finding.failure_scenario).trim(),
    evidence: clip(String(finding.evidence).trim(), 300),
    confidence: Math.min(1, Math.max(0, Number(finding.confidence) || 0)),
  };
};
let droppedOverLimit = 0;
// R5.2.2: keep at most the per-finder limit, most severe and confident first; true per new one.
const takeFindings = (answer, lens, who) => {
  const findings = answer.findings.map(normalize).filter((finding) => finding !== null);
  findings.sort((left, right) => rankOf(left.severity) - rankOf(right.severity) || right.confidence - left.confidence);
  if (findings.length > perFinder) {
    const dropped = findings.length - perFinder;
    droppedOverLimit += dropped;
    log(`${who}: kept ${perFinder} of ${findings.length} findings, dropped ${dropped} over the per-finder limit`);
  }
  let fresh = 0;
  for (const finding of findings.slice(0, perFinder)) {
    if (mergeFinding(finding, lens)) {
      fresh += 1;
    }
  }
  return fresh;
};

const runFinders = async (pairs, round) => {
  const answers = await parallel(pairs.map((pair) => () =>
    ask(`find:${pair.lens}`, `find ${pair.lens} #${pair.group + 1}${round > 1 ? ` r${round}` : ""}`, SCHEMAS.findings, findTask(pair, candidates))));
  let fresh = 0;
  answers.forEach((answer, index) => {
    const pair = pairs[index];
    const who = `find ${pair.lens} #${pair.group + 1} round ${round}`;
    if (answer === null) {
      pair.fresh = 0;
      log(`${who} gave no answer: ${plural(pair.files.length, "file")} not reviewed through ${pair.lens} in this round`);
      return;
    }
    for (const file of pair.files) {
      reviewedPaths.add(file.path);
    }
    pair.fresh = takeFindings(answer, pair.lens, who);
    fresh += pair.fresh;
  });
  if (answers.length > 0 && answers.every((answer) => answer === null)) {
    note(`Find round ${round}: every finder gave no answer`);
  }
  return fresh;
};

phase("Find");
let pairs = [];
for (const lens of plan.lenses) {
  groups.forEach((files, group) => pairs.push({ lens, group, files, fresh: 0 }));
}
log(`Find: ${plural(planned.length, "file")} in ${plural(groups.length, "group")} through ${plan.lenses.join(", ")}`);
await runFinders(pairs, 1);
let roundsRun = 1;
// R5.2.4: further rounds only for pairs whose last round found something new.
for (let round = 2; round <= plan.rounds; round += 1) {
  pairs = pairs.filter((pair) => pair.fresh > 0);
  if (pairs.length === 0) {
    log(`Find: round ${round} not started: round ${round - 1} found nothing new`);
    break;
  }
  const covered = await affordable(pairs.length, 1, `Find round ${round}`);
  if (covered < pairs.length) {
    log(`Find: round ${round} not started: the remaining budget does not cover its ${plural(pairs.length, "finder")}`);
    break;
  }
  roundsRun = round;
  const fresh = await runFinders(pairs, round);
  if (fresh === 0) {
    log(`Find: round ${round} found nothing new; stopping`);
    break;
  }
}
if (roundsRun === plan.rounds && plan.rounds > 1) {
  log(`Find: stopped after the round limit of ${plan.rounds}`);
}

// ---------------------------------------------------------------------------------------
// Verify (UC5.3)
// ---------------------------------------------------------------------------------------

const describe = (candidate) => `- file: ${candidate.file}, line ${candidate.line}${candidate.end_line === null ? "" : ` to ${candidate.end_line}`}
- severity: ${candidate.severity}
- title: ${candidate.title}
- failure scenario: ${candidate.failure_scenario}
- evidence: ${candidate.evidence}`;

const verifyTask = (candidate, skeptic) => `You are an independent skeptic in a multi-agent code review. A reviewer reported the defect below; try hard to refute it: ${SKEPTICS[skeptic]}.
Target: ${scope.target}
Diff of the file: run "${scope.diff_command} ${candidate.file}". ${revisionLine()}
Reported defect:
${describe(candidate)}
Answer verdict "upheld" only if the failure scenario happens with the code as written, "refuted" if it cannot happen or is not a defect, "uncertain" if the code does not let you decide. reason: one or two sentences citing the code. severity: your assessment on the same scale, or null when you refute it. line: the 1-based line where the defect is (the reported one when it is right), or null.
${SEVERITY_SCALE}`;

// R5.3.3, R5.3.4: the verdict of the votes, and the severity and line most voters give.
const judge = (candidate, votes) => {
  const tally = { upheld: 0, refuted: 0, uncertain: 0, missing: 0 };
  for (const vote of votes) {
    tally[vote === null ? "missing" : vote.verdict] += 1;
  }
  const answered = tally.upheld + tally.refuted + tally.uncertain;
  let verdict = "contested";
  if (answered === 0) {
    verdict = "unverified";
  } else if (2 * tally.refuted > answered) {
    verdict = "refuted";
  } else if (tally.upheld === answered) {
    verdict = "confirmed";
  }
  const majority = (values, total, current) => {
    const counts = new Map();
    for (const value of values) {
      if (value !== null && value !== current) {
        counts.set(value, (counts.get(value) ?? 0) + 1);
      }
    }
    for (const [value, count] of counts) {
      if (count * 2 > total) {
        return value;
      }
    }
    return current;
  };
  const answers = votes.filter((vote) => vote !== null);
  const severity = majority(answers.map((vote) => vote.severity), answered, candidate.severity);
  const line = majority(answers.filter((vote) => vote.verdict === "upheld").map((vote) => vote.line), tally.upheld, candidate.line);
  return { candidate, verdict, votes: tally, severity, line };
};

const verify = async (candidate) => {
  const votes = await parallel(plan.skeptics.map((skeptic, index) => () =>
    ask(`verify:${skeptic}`, `verify ${where(candidate)} ${skeptic}${index > 0 && plan.skeptics.indexOf(skeptic) < index ? " 2" : ""}`, SCHEMAS.verdict, verifyTask(candidate, skeptic))));
  return judge(candidate, votes);
};

const outcomes = new Map();
let verifiedCount = 0;
let cappedCount = 0;
const unverified = (candidate) => ({ candidate, verdict: "unverified", votes: { upheld: 0, refuted: 0, uncertain: 0, missing: 0 }, severity: candidate.severity, line: candidate.line });
// R5.3.1, R5.5.3: verify in rank order within the cap and the budget; the rest stay unverified.
const verifyAll = async (pool, what) => {
  const ranked = [...pool].sort(bySeverityAndConfidence);
  const room = Math.max(0, plan.verifyCap - verifiedCount);
  if (ranked.length > room) {
    cappedCount += ranked.length - room;
    log(`${what}: ${plural(ranked.length - room, "candidate")} over the verification cap of ${plan.verifyCap} stay unverified`);
  }
  let queue = ranked.slice(0, room);
  const covered = await affordable(queue.length, plan.skeptics.length, what);
  for (const candidate of [...ranked.slice(room), ...queue.slice(covered)]) {
    outcomes.set(candidate.id, unverified(candidate));
  }
  queue = queue.slice(0, covered);
  verifiedCount += queue.length;
  const results = await pipeline(queue, verify);
  results.forEach((result, index) => {
    outcomes.set(queue[index].id, result ?? unverified(queue[index]));
  });
  if (queue.length > 0 && results.every((result) => result === null || result.verdict === "unverified")) {
    note(`${what}: every skeptic gave no answer`);
  }
};

phase("Verify");
log(`Verify: ${plural(candidates.length, "candidate")}, ${plural(plan.skeptics.length, "vote")} each (${plan.skeptics.join(", ")})`);
await verifyAll(candidates, "Verify");

// ---------------------------------------------------------------------------------------
// Critic (UC5.4)
// ---------------------------------------------------------------------------------------

let criticsRun = 0;
if (plan.critics > 0) {
  phase("Critic");
  const count = await affordable(plan.critics, 1, "Critic");
  const kept = [...outcomes.values()].filter((outcome) => outcome.verdict === "confirmed" || outcome.verdict === "contested");
  const refuted = [...outcomes.values()].filter((outcome) => outcome.verdict === "refuted");
  const unreviewedNow = reviewable.filter((file) => !reviewedPaths.has(file.path));
  const list = (items, render) => (items.length === 0 ? "- none" : items.map(render).join("\n"));
  const criticTask = (index) => `You check the completeness of a multi-agent code review and report defects it missed.
Target: ${scope.target}
Diff of a file: run "${scope.diff_command} <path>". ${revisionLine()}
Changed files:
${reviewable.map(fileLine).join("\n")}
Lenses used: ${plan.lenses.join(", ")}.${focus === "" ? "" : `\nFocus of this review: ${focus}`}
Kept findings:
${list(kept, (outcome) => `- [${outcome.severity}] ${outcome.candidate.title} (${where(outcome.candidate)})`)}
Refuted findings:
${list(refuted, (outcome) => `- ${outcome.candidate.title} (${where(outcome.candidate)})`)}
Files no reviewer read:
${list(unreviewedNow, (file) => `- ${file.path}`)}
Task: ${CRITIC_TASKS[index]} Report only new defects, not the ones listed, at most ${perFinder}, each with a concrete failure scenario. With none answer {"findings": [], "notes": null}.
${SEVERITY_SCALE}`;
  const answers = await parallel(Array.from({ length: count }, (_, index) => () =>
    ask(`critic:${index + 1}`, `critic ${index + 1}`, SCHEMAS.findings, criticTask(index))));
  criticsRun = count;
  const before = candidates.length;
  answers.forEach((answer, index) => {
    if (answer === null) {
      log(`critic ${index + 1} gave no answer`);
      return;
    }
    takeFindings(answer, "critic", `critic ${index + 1}`);
  });
  if (answers.length > 0 && answers.every((answer) => answer === null)) {
    note("Critic: every critic gave no answer");
  }
  const fresh = candidates.slice(before);
  log(`Critic: ${plural(fresh.length, "new candidate")}`);
  // R5.4.2: only the new candidates go through the same verification, within the cap left.
  await verifyAll(fresh, "Verify critic findings");
}

// ---------------------------------------------------------------------------------------
// Report (UC5.5)
// ---------------------------------------------------------------------------------------

phase("Report");
const omitted = emptyOmitted();
const shown = [];
for (const candidate of candidates) {
  const outcome = outcomes.get(candidate.id) ?? unverified(candidate);
  if (outcome.verdict === "unverified") {
    omitted.unverified += 1;
    if (omitted.unverified_titles.length < MAX_UNVERIFIED_TITLES) {
      omitted.unverified_titles.push(`${candidate.title} (${where(candidate)})`);
    }
  } else if (outcome.verdict === "refuted") {
    omitted.refuted += 1;
  } else if (!row.verdicts.includes(outcome.verdict)) {
    omitted.contested += 1;
  } else if (!row.severities.includes(outcome.severity)) {
    omitted.below_severity += 1;
  } else {
    shown.push(outcome);
  }
}
const verdictRank = (verdict) => (verdict === "confirmed" ? 0 : 1);
shown.sort((left, right) =>
  rankOf(left.severity) - rankOf(right.severity) ||
  verdictRank(left.verdict) - verdictRank(right.verdict) ||
  compare(left.candidate.file, right.candidate.file) ||
  left.line - right.line);
const cap = a.max_findings ?? row.maxFindings;
const limit = cap === "all" ? shown.length : cap;
omitted.over_cap = Math.max(0, shown.length - limit);
const findings = shown.slice(0, limit).map((outcome, index) => ({
  id: `F${index + 1}`,
  file: outcome.candidate.file,
  line: outcome.line,
  end_line: outcome.candidate.end_line === null ? null : Math.max(outcome.line, outcome.candidate.end_line),
  severity: outcome.severity,
  title: outcome.candidate.title,
  failure_scenario: outcome.candidate.failure_scenario,
  evidence: outcome.candidate.evidence,
  verdict: outcome.verdict,
  votes: outcome.votes,
  lenses: outcome.candidate.lenses,
}));

if (droppedOverLimit > 0) {
  notes.push(`${plural(droppedOverLimit, "finding")} over the per-finder limit of ${perFinder} dropped before merging`);
}
const unreviewed = reviewable.filter((file) => !reviewedPaths.has(file.path)).map((file) => file.path);
if (unreviewed.length > 0) {
  log(`unreviewed: ${unreviewed.slice(0, MAX_UNREVIEWED_LISTED).join(", ")}${unreviewed.length > MAX_UNREVIEWED_LISTED ? ", …" : ""}`);
}
const bySeverity = SEVERITIES
  .map((severity) => [severity, findings.filter((finding) => finding.severity === severity).length])
  .filter(([, count]) => count > 0)
  .map(([severity, count]) => `${count} ${severity}`)
  .join(", ");
const omittedParts = [
  [omitted.refuted, "refuted"],
  [omitted.contested, "contested"],
  [omitted.below_severity, "below the reported severity"],
  [omitted.over_cap, `over the cap of ${limit}`],
  [omitted.unverified, cappedCount > 0 ? `unverified (verification cap ${plan.verifyCap})` : "unverified"],
].filter(([count]) => count > 0).map(([count, what]) => `${count} ${what}`);
let summary = `${plural(findings.length, "finding")}${findings.length > 0 ? ` (${bySeverity})` : ""} in ${reviewedPaths.size} of ${plural(changed.length, "file")}`;
if (omittedParts.length > 0) {
  summary += `; omitted: ${omittedParts.join(", ")}`;
}
if (unreviewed.length > 0) {
  summary += `; ${plural(unreviewed.length, "file")} unreviewed${plan.reasons.length > 0 ? ` (plan reduced to fit ${plan.reasons.join(" and ")})` : ""}`;
}
log(summary);

return {
  workflow: "review",
  version: 1,
  target: scope.target,
  effort: level,
  summary,
  findings,
  omitted,
  coverage: {
    files_total: changed.length,
    files_reviewed: reviewedPaths.size,
    unreviewed: unreviewed.slice(0, MAX_UNREVIEWED_LISTED),
    unreviewed_total: unreviewed.length,
    lenses: plan.lenses,
    rounds: roundsRun,
    critics: criticsRun,
    notes,
  },
};
