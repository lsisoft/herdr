use std::path::Path;

use serde::{Deserialize, Serialize};

const MAX_SESSION_ID_LEN: usize = 512;
const MAX_SESSION_PATH_LEN: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionRef {
    pub kind: AgentSessionRefKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionRefKind {
    Id,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentResumePlan {
    pub agent: String,
    pub argv: Vec<String>,
    pub dedupe_key: String,
}

/// Allow-listed, non-secret permission arguments that can be reapplied after
/// a native session restore. Arbitrary argv, config overrides, and environment
/// values are intentionally excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", content = "args", rename_all = "snake_case")]
pub enum AgentResumeAccess {
    Codex(Vec<String>),
    Claude(Vec<String>),
    Copilot(Vec<String>),
    Opencode(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedAgentSession {
    pub source: String,
    pub agent: String,
    pub session_ref: AgentSessionRef,
}

impl AgentSessionRef {
    pub fn id(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        valid_session_id(&value).then_some(Self {
            kind: AgentSessionRefKind::Id,
            value,
        })
    }

    pub fn path(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        valid_session_path(&value).then_some(Self {
            kind: AgentSessionRefKind::Path,
            value,
        })
    }
}

pub fn session_ref_from_report(
    source: &str,
    agent: &str,
    agent_session_id: Option<String>,
    _agent_session_path: Option<String>,
) -> Option<AgentSessionRef> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    if agent == "pi" || agent == "omp" {
        return _agent_session_path
            .and_then(AgentSessionRef::path)
            .or_else(|| agent_session_id.and_then(AgentSessionRef::id));
    }

    agent_session_id.and_then(AgentSessionRef::id)
}

pub fn normalize_session_start_source(value: Option<String>) -> Option<String> {
    match value.as_deref().map(str::trim) {
        Some(source @ ("startup" | "resume" | "clear" | "compact" | "new" | "fork")) => {
            Some(source.to_string())
        }
        _ => None,
    }
}

pub fn is_reserved_native_state_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("herdr:claude", "claude")
            | ("herdr:codex", "codex")
            | ("herdr:copilot", "copilot")
            | ("herdr:devin", "devin")
            | ("herdr:droid", "droid")
            | ("herdr:qodercli", "qodercli")
            | ("herdr:cursor", "cursor")
    )
}

pub fn session_ref_from_snapshot(
    source: &str,
    agent: &str,
    kind: AgentSessionRefKind,
    value: &str,
) -> Option<PersistedAgentSession> {
    if !is_official_agent_source(source, agent) {
        return None;
    }
    let session_ref = match (agent, kind) {
        ("pi" | "omp", AgentSessionRefKind::Path) => AgentSessionRef::path(value)?,
        (_, AgentSessionRefKind::Id) => AgentSessionRef::id(value)?,
        _ => return None,
    };
    Some(PersistedAgentSession {
        source: source.to_string(),
        agent: agent.to_string(),
        session_ref,
    })
}

pub fn plan(source: &str, agent: &str, session_ref: &AgentSessionRef) -> Option<AgentResumePlan> {
    plan_with_access(source, agent, session_ref, None)
}

pub fn plan_with_access(
    source: &str,
    agent: &str,
    session_ref: &AgentSessionRef,
    access: Option<&AgentResumeAccess>,
) -> Option<AgentResumePlan> {
    plan_with_profile(source, agent, session_ref, access, None)
}

pub fn plan_with_profile(
    source: &str,
    agent: &str,
    session_ref: &AgentSessionRef,
    access: Option<&AgentResumeAccess>,
    runtime: Option<&crate::agent_runtime::AgentRuntimeSettings>,
) -> Option<AgentResumePlan> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    let argv = match (source, agent, session_ref.kind) {
        ("herdr:claude", "claude", AgentSessionRefKind::Id) => {
            vec![
                "claude".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:codex", "codex", AgentSessionRefKind::Id) => {
            vec!["codex".into(), "resume".into(), session_ref.value.clone()]
        }
        ("herdr:copilot", "copilot", AgentSessionRefKind::Id) => {
            vec!["copilot".into(), format!("--resume={}", session_ref.value)]
        }
        ("herdr:devin", "devin", AgentSessionRefKind::Id) => {
            vec!["devin".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("herdr:droid", "droid", AgentSessionRefKind::Id) => {
            vec!["droid".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("herdr:kimi", "kimi", AgentSessionRefKind::Id) => {
            vec!["kimi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("herdr:mastracode", "mastracode", AgentSessionRefKind::Id) => {
            vec![
                "mastracode".into(),
                "--thread".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:pi", "pi", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            vec!["pi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("herdr:omp", "omp", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            // omp resume is `-r, --resume=<value>` (ID prefix or path); it has no
            // `--session` flag, unlike pi.
            vec!["omp".into(), format!("--resume={}", session_ref.value)]
        }
        ("herdr:hermes", "hermes", AgentSessionRefKind::Id) => {
            vec![
                "hermes".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:opencode", "opencode", AgentSessionRefKind::Id) => {
            vec![
                "opencode".into(),
                "--session".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:qodercli", "qodercli", AgentSessionRefKind::Id) => {
            vec![
                "qodercli".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("herdr:kilo", "kilo", AgentSessionRefKind::Id) => {
            vec!["kilo".into(), "--session".into(), session_ref.value.clone()]
        }
        ("herdr:cursor", "cursor", AgentSessionRefKind::Id) => {
            vec![
                "cursor-agent".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        _ => return None,
    };

    let mut argv = argv;
    if let Some(access_args) = access.and_then(|access| access.args_for(source, agent)) {
        argv.extend(access_args);
    }
    if let Some(runtime) = runtime {
        argv.extend(runtime.resume_args(source, agent));
    }

    Some(AgentResumePlan {
        agent: agent.to_string(),
        argv,
        dedupe_key: dedupe_key(source, agent, session_ref),
    })
}

impl AgentResumeAccess {
    fn args_for(&self, source: &str, agent: &str) -> Option<Vec<String>> {
        let (executable, args) = match (source, agent, self) {
            ("herdr:codex", "codex", Self::Codex(args)) => ("codex", args),
            ("herdr:claude", "claude", Self::Claude(args)) => ("claude", args),
            ("herdr:copilot", "copilot", Self::Copilot(args)) => ("copilot", args),
            ("herdr:opencode", "opencode", Self::Opencode(args)) => ("opencode", args),
            _ => return None,
        };
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(executable.to_string());
        argv.extend(args.iter().cloned());
        match access_from_argv(source, agent, &argv)? {
            Self::Codex(args) | Self::Claude(args) | Self::Copilot(args) | Self::Opencode(args) => {
                Some(args)
            }
        }
    }
}

/// Extract documented permission-related CLI arguments for supported native
/// resume providers. Unknown arguments are omitted, so a restore cannot gain
/// access from an arbitrary launch command.
pub fn access_from_argv(source: &str, agent: &str, argv: &[String]) -> Option<AgentResumeAccess> {
    let executable = match (source, agent) {
        ("herdr:codex", "codex") => "codex",
        ("herdr:claude", "claude") => "claude",
        ("herdr:copilot", "copilot") => "copilot",
        ("herdr:opencode", "opencode") => "opencode",
        _ => return None,
    };
    let start = agent_executable_position(argv, executable)?;
    let args = match executable {
        "codex" => collect_access_args(
            &argv[start + 1..],
            &["--sandbox", "-s", "--ask-for-approval", "-a", "--add-dir"],
            &[],
            &[
                "--yolo",
                "--dangerously-bypass-approvals-and-sandbox",
                "--dangerously-bypass-hook-trust",
                "--search",
            ],
        ),
        "claude" => collect_access_args(
            &argv[start + 1..],
            &["--permission-mode", "--permission-prompt-tool", "--tools"],
            &[
                "--add-dir",
                "--allowedTools",
                "--allowed-tools",
                "--disallowedTools",
                "--disallowed-tools",
            ],
            &[
                "--dangerously-skip-permissions",
                "--allow-dangerously-skip-permissions",
            ],
        ),
        "copilot" => collect_access_args(
            &argv[start + 1..],
            &[
                "--add-dir",
                "--allow-tool",
                "--deny-tool",
                "--allow-url",
                "--deny-url",
                "--available-tools",
                "--excluded-tools",
                "--disable-mcp-server",
            ],
            &[],
            &[
                "--allow-all-tools",
                "--allow-all-paths",
                "--allow-all-urls",
                "--allow-all",
                "--yolo",
                "--allow-all-mcp-server-instructions",
                "--enable-all-github-mcp-tools",
                "--disallow-temp-dir",
                "--disable-builtin-mcps",
                "--no-ask-user",
            ],
        ),
        "opencode" => collect_access_args(&argv[start + 1..], &[], &[], &["--auto"]),
        _ => Vec::new(),
    };
    (!args.is_empty()).then_some(match executable {
        "codex" => AgentResumeAccess::Codex(args),
        "claude" => AgentResumeAccess::Claude(args),
        "copilot" => AgentResumeAccess::Copilot(args),
        "opencode" => AgentResumeAccess::Opencode(args),
        _ => unreachable!(),
    })
}

pub(crate) fn agent_executable_position(argv: &[String], executable: &str) -> Option<usize> {
    argv.iter().position(|arg| {
        let name = arg.rsplit(['/', '\\']).next().unwrap_or(arg);
        if name == executable {
            return true;
        }
        ["exe", "cmd", "bat"]
            .into_iter()
            .any(|extension| name.eq_ignore_ascii_case(&format!("{executable}.{extension}")))
    })
}

fn collect_access_args(
    argv: &[String],
    value_flags: &[&str],
    variadic_value_flags: &[&str],
    boolean_flags: &[&str],
) -> Vec<String> {
    let mut collected = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if arg == "--" {
            break;
        }
        if boolean_flags.contains(&arg.as_str()) {
            collected.push(arg.clone());
        } else if let Some((flag, value)) = arg.split_once('=') {
            if (value_flags.contains(&flag) || variadic_value_flags.contains(&flag))
                && valid_access_value(value)
            {
                collected.push(arg.clone());
            }
        } else if variadic_value_flags.contains(&arg.as_str()) {
            let values_start = index + 1;
            let mut next = values_start;
            while next < argv.len()
                && argv[next] != "--"
                && !argv[next].starts_with('-')
                && valid_access_value(&argv[next])
            {
                next += 1;
            }
            if next > values_start {
                collected.push(arg.clone());
                collected.extend(argv[values_start..next].iter().cloned());
                index = next - 1;
            }
        } else if value_flags.contains(&arg.as_str()) {
            if let Some(value) = argv
                .get(index + 1)
                .filter(|value| valid_access_value(value))
            {
                collected.push(arg.clone());
                collected.push(value.clone());
                index += 1;
            }
        }
        index += 1;
    }
    collected
}

fn valid_access_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

pub fn dedupe_key(source: &str, agent: &str, session_ref: &AgentSessionRef) -> String {
    format!(
        "{source}\u{0}{agent}\u{0}{:?}\u{0}{}",
        session_ref.kind, session_ref.value
    )
}

fn is_official_agent_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("herdr:claude", "claude")
            | ("herdr:codex", "codex")
            | ("herdr:copilot", "copilot")
            | ("herdr:devin", "devin")
            | ("herdr:droid", "droid")
            | ("herdr:kimi", "kimi")
            | ("herdr:omp", "omp")
            | ("herdr:mastracode", "mastracode")
            | ("herdr:pi", "pi")
            | ("herdr:hermes", "hermes")
            | ("herdr:opencode", "opencode")
            | ("herdr:qodercli", "qodercli")
            | ("herdr:kilo", "kilo")
            | ("herdr:cursor", "cursor")
    )
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_ID_LEN
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

fn valid_session_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_PATH_LEN
        && !value.chars().any(char::is_control)
        && Path::new(value).is_absolute()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute_test_path(name: &str) -> String {
        std::env::current_dir()
            .unwrap()
            .join(name)
            .display()
            .to_string()
    }

    #[test]
    fn native_state_reservation_excludes_full_lifecycle_sources() {
        assert!(is_reserved_native_state_source("herdr:claude", "claude"));
        assert!(is_reserved_native_state_source("herdr:codex", "codex"));
        assert!(is_reserved_native_state_source("herdr:devin", "devin"));
        assert!(!is_reserved_native_state_source("herdr:kimi", "kimi"));
        assert!(!is_reserved_native_state_source(
            "herdr:opencode",
            "opencode"
        ));
    }

    #[test]
    fn planner_allows_supported_agents() {
        let pi_session = absolute_test_path("pi-session.jsonl");
        let omp_session = absolute_test_path("omp-session.jsonl");
        assert_eq!(
            plan(
                "herdr:claude",
                "claude",
                &AgentSessionRef::id("claude-session").unwrap(),
            )
            .unwrap()
            .argv,
            vec!["claude", "--resume", "claude-session"]
        );
        assert_eq!(
            plan(
                "herdr:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap(),
            )
            .unwrap()
            .argv,
            vec!["codex", "resume", "codex-session"]
        );
        assert_eq!(
            plan(
                "herdr:copilot",
                "copilot",
                &AgentSessionRef::id("copilot-session").unwrap(),
            )
            .unwrap()
            .argv,
            vec!["copilot", "--resume=copilot-session"]
        );
        assert_eq!(
            plan(
                "herdr:devin",
                "devin",
                &AgentSessionRef::id("devin-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["devin", "--resume", "devin-session"]
        );
        assert_eq!(
            plan(
                "herdr:droid",
                "droid",
                &AgentSessionRef::id("droid-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["droid", "--resume", "droid-session"]
        );
        assert_eq!(
            plan(
                "herdr:kimi",
                "kimi",
                &AgentSessionRef::id("kimi-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["kimi", "--session", "kimi-session"]
        );
        assert_eq!(
            plan(
                "herdr:mastracode",
                "mastracode",
                &AgentSessionRef::id("mastracode-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["mastracode", "--thread", "mastracode-session"]
        );
        assert_eq!(
            plan(
                "herdr:pi",
                "pi",
                &AgentSessionRef::path(&pi_session).unwrap()
            )
            .unwrap()
            .argv,
            vec!["pi", "--session", pi_session.as_str()]
        );
        assert_eq!(
            plan(
                "herdr:omp",
                "omp",
                &AgentSessionRef::path(&omp_session).unwrap()
            )
            .unwrap()
            .argv,
            vec!["omp", format!("--resume={omp_session}").as_str()]
        );
        assert_eq!(
            plan(
                "herdr:hermes",
                "hermes",
                &AgentSessionRef::id("hermes-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["hermes", "--resume", "hermes-session"]
        );
        assert_eq!(
            plan(
                "herdr:opencode",
                "opencode",
                &AgentSessionRef::id("opencode-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["opencode", "--session", "opencode-session"]
        );
        assert_eq!(
            plan(
                "herdr:qodercli",
                "qodercli",
                &AgentSessionRef::id("qoder-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["qodercli", "--resume", "qoder-session"]
        );
        assert_eq!(
            plan(
                "herdr:kilo",
                "kilo",
                &AgentSessionRef::id("kilo-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["kilo", "--session", "kilo-session"]
        );
        assert_eq!(
            plan(
                "herdr:cursor",
                "cursor",
                &AgentSessionRef::id("cursor-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["cursor-agent", "--resume", "cursor-session"]
        );
    }

    #[test]
    fn planner_rejects_custom_and_unsupported_path_refs() {
        let claude_session = absolute_test_path("claude-session");
        assert!(plan(
            "custom:claude",
            "claude",
            &AgentSessionRef::id("session").unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:claude",
            "claude",
            &AgentSessionRef::path(&claude_session).unwrap()
        )
        .is_none());
    }

    #[test]
    fn report_ref_prefers_pi_and_omp_paths_and_validates_values() {
        let pi_session = absolute_test_path("pi-session.jsonl");
        let omp_session = absolute_test_path("omp-session.jsonl");
        let claude_session = absolute_test_path("claude-session");
        let copilot_session = absolute_test_path("copilot-session");
        let session_ref = session_ref_from_report(
            "herdr:pi",
            "pi",
            Some("pi-id".into()),
            Some(pi_session.clone()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(session_ref.value, pi_session);

        assert!(session_ref_from_report("herdr:pi", "pi", Some("bad\nid".into()), None).is_none());
        assert!(
            session_ref_from_report("herdr:pi", "pi", None, Some("relative.jsonl".into()))
                .is_none()
        );
        assert!(session_ref_from_report("custom:pi", "pi", Some("pi-id".into()), None).is_none());

        let session_ref = session_ref_from_report(
            "herdr:omp",
            "omp",
            Some("omp-id".into()),
            Some(omp_session.clone()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(session_ref.value, omp_session);

        let session_ref =
            session_ref_from_report("herdr:omp", "omp", Some("omp-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "omp-id");
        let session_ref = session_ref_from_report(
            "herdr:omp",
            "omp",
            Some("omp-id".into()),
            Some("relative.jsonl".into()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "omp-id");
        assert!(
            session_ref_from_report("herdr:omp", "omp", None, Some("relative.jsonl".into()))
                .is_none()
        );

        assert!(
            session_ref_from_report("herdr:claude", "claude", None, Some(claude_session)).is_none()
        );

        let session_ref =
            session_ref_from_report("herdr:copilot", "copilot", Some("copilot-id".into()), None)
                .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "copilot-id");
        assert!(
            session_ref_from_report("herdr:copilot", "copilot", None, Some(copilot_session))
                .is_none()
        );

        let session_ref =
            session_ref_from_report("herdr:devin", "devin", Some("devin-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "devin-id");

        let session_ref =
            session_ref_from_report("herdr:droid", "droid", Some("droid-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "droid-id");
        assert!(session_ref_from_report(
            "herdr:droid",
            "droid",
            None,
            Some("/tmp/droid-session".into())
        )
        .is_none());

        let session_ref =
            session_ref_from_report("herdr:kimi", "kimi", Some("kimi-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "kimi-id");

        let session_ref = session_ref_from_report(
            "herdr:mastracode",
            "mastracode",
            Some("mastracode-id".into()),
            None,
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "mastracode-id");

        let session_ref =
            session_ref_from_report("herdr:kilo", "kilo", Some("kilo-id".into()), None).unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "kilo-id");

        let session_ref =
            session_ref_from_report("herdr:qodercli", "qodercli", Some("qoder-id".into()), None)
                .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Id);
        assert_eq!(session_ref.value, "qoder-id");
    }

    #[test]
    fn normalize_session_start_source_allows_known_values() {
        assert_eq!(
            normalize_session_start_source(Some("startup".into())),
            Some("startup".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("resume".into())),
            Some("resume".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("clear".into())),
            Some("clear".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("compact".into())),
            Some("compact".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("new".into())),
            Some("new".into())
        );
        assert_eq!(
            normalize_session_start_source(Some("fork".into())),
            Some("fork".into())
        );
        assert_eq!(
            normalize_session_start_source(Some(" resume ".into())),
            Some("resume".into())
        );
        assert_eq!(normalize_session_start_source(Some("other".into())), None);
        assert_eq!(normalize_session_start_source(None), None);
    }

    #[test]
    fn ids_are_data_not_shell_text() {
        let id = "abc; rm -rf /";
        let codex_plan = plan("herdr:codex", "codex", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(codex_plan.argv, vec!["codex", "resume", id]);

        let copilot_plan = plan(
            "herdr:copilot",
            "copilot",
            &AgentSessionRef::id(id).unwrap(),
        )
        .unwrap();
        assert_eq!(copilot_plan.argv, vec!["copilot", "--resume=abc; rm -rf /"]);

        let devin_plan = plan("herdr:devin", "devin", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(devin_plan.argv, vec!["devin", "--resume", id]);
    }

    #[test]
    fn access_profiles_keep_only_documented_permission_arguments() {
        let cases = [
            (
                "herdr:codex",
                "codex",
                vec![
                    "node",
                    "/opt/bin/codex",
                    "--yolo",
                    "--sandbox",
                    "workspace-write",
                    "--model",
                    "ignored",
                ],
                AgentResumeAccess::Codex(vec![
                    "--yolo".into(),
                    "--sandbox".into(),
                    "workspace-write".into(),
                ]),
            ),
            (
                "herdr:claude",
                "claude",
                vec![
                    "claude",
                    "--dangerously-skip-permissions",
                    "--allow-dangerously-skip-permissions",
                    "--allowed-tools",
                    "Bash(git status:*)",
                    "Read",
                    "--tools",
                    "Bash,Read,Edit",
                    "--model",
                    "ignored",
                ],
                AgentResumeAccess::Claude(vec![
                    "--dangerously-skip-permissions".into(),
                    "--allow-dangerously-skip-permissions".into(),
                    "--allowed-tools".into(),
                    "Bash(git status:*)".into(),
                    "Read".into(),
                    "--tools".into(),
                    "Bash,Read,Edit".into(),
                ]),
            ),
            (
                "herdr:copilot",
                "copilot",
                vec![
                    "copilot",
                    "--allow-tool=shell(git:*)",
                    "--deny-tool",
                    "shell(git push)",
                    "--add-dir=../shared",
                    "--no-ask-user",
                    "--disallow-temp-dir",
                    "--model",
                    "ignored",
                ],
                AgentResumeAccess::Copilot(vec![
                    "--allow-tool=shell(git:*)".into(),
                    "--deny-tool".into(),
                    "shell(git push)".into(),
                    "--add-dir=../shared".into(),
                    "--no-ask-user".into(),
                    "--disallow-temp-dir".into(),
                ]),
            ),
            (
                "herdr:opencode",
                "opencode",
                vec!["opencode", "--auto", "--model", "ignored"],
                AgentResumeAccess::Opencode(vec!["--auto".into()]),
            ),
        ];

        for (source, agent, argv, expected) in cases {
            let argv = argv.into_iter().map(str::to_string).collect::<Vec<_>>();
            assert_eq!(access_from_argv(source, agent, &argv), Some(expected));
        }
    }

    #[test]
    fn planner_appends_matching_access_profile_only() {
        let access = AgentResumeAccess::Codex(vec!["--yolo".into()]);
        assert_eq!(
            plan_with_access(
                "herdr:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap(),
                Some(&access),
            )
            .unwrap()
            .argv,
            vec!["codex", "resume", "codex-session", "--yolo"]
        );
        assert_eq!(
            plan_with_access(
                "herdr:claude",
                "claude",
                &AgentSessionRef::id("claude-session").unwrap(),
                Some(&access),
            )
            .unwrap()
            .argv,
            vec!["claude", "--resume", "claude-session"]
        );
    }

    #[test]
    fn planner_revalidates_persisted_access_arguments() {
        let tampered =
            AgentResumeAccess::Codex(vec!["--sandbox".into(), "--model".into(), "ignored".into()]);

        assert_eq!(
            plan_with_access(
                "herdr:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap(),
                Some(&tampered),
            )
            .unwrap()
            .argv,
            vec!["codex", "resume", "codex-session"]
        );
        assert!(AgentSessionRef::id("--dangerously-skip-permissions").is_none());
    }

    #[test]
    fn planner_appends_runtime_after_access_profile() {
        let access = AgentResumeAccess::Codex(vec!["--yolo".into()]);
        let runtime = crate::agent_runtime::AgentRuntimeSettings {
            model: Some("gpt-5.6-sol".into()),
            reasoning_effort: Some("max".into()),
            ..crate::agent_runtime::AgentRuntimeSettings::default()
        };
        assert_eq!(
            plan_with_profile(
                "herdr:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap(),
                Some(&access),
                Some(&runtime),
            )
            .unwrap()
            .argv,
            vec![
                "codex",
                "resume",
                "codex-session",
                "--yolo",
                "--model",
                "gpt-5.6-sol",
                "--config",
                "model_reasoning_effort=\"max\"",
            ]
        );
    }

    #[test]
    fn access_capture_recognizes_windows_launchers() {
        assert_eq!(
            access_from_argv(
                "herdr:codex",
                "codex",
                &["C:\\tools\\codex.exe".into(), "--yolo".into()],
            ),
            Some(AgentResumeAccess::Codex(vec!["--yolo".into()]))
        );
    }

    #[test]
    fn planner_rejects_path_refs_for_id_only_agents() {
        let hermes_session = absolute_test_path("hermes-session");
        let opencode_session = absolute_test_path("opencode-session");
        let kilo_session = absolute_test_path("kilo-session");
        let copilot_session = absolute_test_path("copilot-session");
        let devin_session = absolute_test_path("devin-session");
        assert!(plan(
            "herdr:hermes",
            "hermes",
            &AgentSessionRef::path(&hermes_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:opencode",
            "opencode",
            &AgentSessionRef::path(&opencode_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:kilo",
            "kilo",
            &AgentSessionRef::path(&kilo_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:copilot",
            "copilot",
            &AgentSessionRef::path(&copilot_session).unwrap()
        )
        .is_none());
        assert!(plan(
            "herdr:devin",
            "devin",
            &AgentSessionRef::path(&devin_session).unwrap()
        )
        .is_none());
        assert!(session_ref_from_snapshot(
            "herdr:mastracode",
            "mastracode",
            AgentSessionRefKind::Id,
            "mastracode-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:hermes",
            "hermes",
            AgentSessionRefKind::Id,
            "hermes-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:opencode",
            "opencode",
            AgentSessionRefKind::Id,
            "opencode-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:kilo",
            "kilo",
            AgentSessionRefKind::Id,
            "kilo-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:copilot",
            "copilot",
            AgentSessionRefKind::Id,
            "copilot-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "herdr:devin",
            "devin",
            AgentSessionRefKind::Id,
            "devin-session"
        )
        .is_some());
    }
}
