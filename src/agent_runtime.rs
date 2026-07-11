use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

const MAX_RUNTIME_VALUE_LEN: usize = 4096;
const MAX_RUNTIME_EVENT_BYTES: usize = 1024 * 1024;

/// Effective model and compute settings that can be observed without storing
/// credentials or arbitrary provider configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentRuntimeSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile: Option<String>,
}

impl AgentRuntimeSettings {
    pub fn merged(mut self, newer: Self) -> Self {
        merge_field(&mut self.model, newer.model);
        merge_field(&mut self.model_provider, newer.model_provider);
        merge_field(&mut self.reasoning_effort, newer.reasoning_effort);
        merge_field(&mut self.reasoning_summary, newer.reasoning_summary);
        merge_field(&mut self.verbosity, newer.verbosity);
        merge_field(&mut self.service_tier, newer.service_tier);
        merge_field(&mut self.context_tier, newer.context_tier);
        merge_field(&mut self.variant, newer.variant);
        merge_field(&mut self.agent_profile, newer.agent_profile);
        self
    }

    pub fn validated(mut self, source: &str, agent: &str) -> Option<Self> {
        sanitize_field(&mut self.model);
        sanitize_field(&mut self.model_provider);
        sanitize_field(&mut self.reasoning_effort);
        sanitize_field(&mut self.reasoning_summary);
        sanitize_field(&mut self.verbosity);
        sanitize_field(&mut self.service_tier);
        sanitize_field(&mut self.context_tier);
        sanitize_field(&mut self.variant);
        sanitize_field(&mut self.agent_profile);

        match (source, agent) {
            ("herdr:codex", "codex") => {
                self.context_tier = None;
                self.variant = None;
                self.agent_profile = None;
            }
            ("herdr:claude", "claude") => {
                self.model_provider = None;
                self.reasoning_summary = None;
                self.verbosity = None;
                self.service_tier = None;
                self.context_tier = None;
                self.variant = None;
            }
            ("herdr:copilot", "copilot") => {
                self.model_provider = None;
                self.service_tier = None;
                self.variant = None;
            }
            ("herdr:opencode", "opencode") => {
                self.model_provider = None;
                self.reasoning_effort = None;
                self.reasoning_summary = None;
                self.verbosity = None;
                self.service_tier = None;
                self.context_tier = None;
            }
            _ => return None,
        }

        (!self.is_empty()).then_some(self)
    }

    pub fn resume_args(&self, source: &str, agent: &str) -> Vec<String> {
        let Some(settings) = self.clone().validated(source, agent) else {
            return Vec::new();
        };
        let mut args = Vec::new();
        match (source, agent) {
            ("herdr:codex", "codex") => {
                push_pair(&mut args, "--model", settings.model);
                push_codex_config(&mut args, "model_provider", settings.model_provider);
                push_codex_config(
                    &mut args,
                    "model_reasoning_effort",
                    settings.reasoning_effort,
                );
                push_codex_config(
                    &mut args,
                    "model_reasoning_summary",
                    settings.reasoning_summary,
                );
                push_codex_config(&mut args, "model_verbosity", settings.verbosity);
                push_codex_config(&mut args, "service_tier", settings.service_tier);
            }
            ("herdr:claude", "claude") => {
                push_pair(&mut args, "--model", settings.model);
                push_pair(&mut args, "--effort", settings.reasoning_effort);
                push_pair(&mut args, "--agent", settings.agent_profile);
            }
            ("herdr:copilot", "copilot") => {
                push_equals(&mut args, "--model", settings.model);
                push_equals(&mut args, "--effort", settings.reasoning_effort);
                push_pair(&mut args, "--context", settings.context_tier);
                push_equals(&mut args, "--agent", settings.agent_profile);
                if settings
                    .reasoning_summary
                    .as_deref()
                    .is_some_and(|value| !matches!(value, "none" | "off" | "disabled" | "false"))
                {
                    args.push("--enable-reasoning-summaries".to_string());
                }
            }
            ("herdr:opencode", "opencode") => {
                push_pair(&mut args, "--model", settings.model);
                // OpenCode documents --variant for `run`, not for TUI session resume.
                push_pair(&mut args, "--agent", settings.agent_profile);
            }
            _ => {}
        }
        args
    }

    fn is_empty(&self) -> bool {
        self.model.is_none()
            && self.model_provider.is_none()
            && self.reasoning_effort.is_none()
            && self.reasoning_summary.is_none()
            && self.verbosity.is_none()
            && self.service_tier.is_none()
            && self.context_tier.is_none()
            && self.variant.is_none()
            && self.agent_profile.is_none()
    }
}

fn merge_field(current: &mut Option<String>, newer: Option<String>) {
    if newer.is_some() {
        *current = newer;
    }
}

fn sanitize_field(value: &mut Option<String>) {
    if value.as_deref().is_none_or(valid_runtime_value) {
        return;
    }
    *value = None;
}

fn valid_runtime_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RUNTIME_VALUE_LEN
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

fn push_pair(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value);
    }
}

fn push_equals(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        args.push(format!("{flag}={value}"));
    }
}

fn push_codex_config(args: &mut Vec<String>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        let encoded = serde_json::to_string(&value).unwrap_or(value);
        args.push("--config".to_string());
        args.push(format!("{key}={encoded}"));
    }
}

/// Extract documented model and compute flags from the live command line.
/// Unknown flags and arbitrary config overrides are ignored.
pub fn from_argv(source: &str, agent: &str, argv: &[String]) -> Option<AgentRuntimeSettings> {
    let executable = match (source, agent) {
        ("herdr:codex", "codex") => "codex",
        ("herdr:claude", "claude") => "claude",
        ("herdr:copilot", "copilot") => "copilot",
        ("herdr:opencode", "opencode") => "opencode",
        _ => return None,
    };
    let start = crate::agent_resume::agent_executable_position(argv, executable)?;
    let args = &argv[start + 1..];
    let mut settings = AgentRuntimeSettings::default();

    match executable {
        "codex" => {
            settings.model = last_flag_value(args, &["--model", "-m"]);
            for override_value in all_flag_values(args, &["--config", "-c"]) {
                let Some((key, value)) = override_value.split_once('=') else {
                    continue;
                };
                let value = parse_config_value(value);
                match key {
                    "model" => settings.model = value,
                    "model_provider" => settings.model_provider = value,
                    "model_reasoning_effort" => settings.reasoning_effort = value,
                    "model_reasoning_summary" => settings.reasoning_summary = value,
                    "model_verbosity" => settings.verbosity = value,
                    "service_tier" => settings.service_tier = value,
                    _ => {}
                }
            }
        }
        "claude" => {
            settings.model = last_flag_value(args, &["--model"]);
            settings.reasoning_effort = last_flag_value(args, &["--effort"]);
            settings.agent_profile = last_flag_value(args, &["--agent"]);
        }
        "copilot" => {
            settings.model = last_flag_value(args, &["--model"]);
            settings.reasoning_effort = last_flag_value(args, &["--effort", "--reasoning-effort"]);
            settings.context_tier = last_flag_value(args, &["--context"]);
            settings.agent_profile = last_flag_value(args, &["--agent"]);
            if args.iter().any(|arg| arg == "--enable-reasoning-summaries") {
                settings.reasoning_summary = Some("enabled".to_string());
            }
        }
        "opencode" => {
            settings.model = last_flag_value(args, &["--model", "-m"]);
            settings.variant = last_flag_value(args, &["--variant"]);
            settings.agent_profile = last_flag_value(args, &["--agent"]);
        }
        _ => {}
    }

    settings.validated(source, agent)
}

fn last_flag_value(args: &[String], flags: &[&str]) -> Option<String> {
    all_flag_values(args, flags).into_iter().last()
}

fn all_flag_values(args: &[String], flags: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            break;
        }
        if let Some((flag, value)) = arg.split_once('=') {
            if flags.contains(&flag) && valid_runtime_value(value) {
                values.push(value.to_string());
            }
        } else if flags.contains(&arg.as_str()) {
            if let Some(value) = args
                .get(index + 1)
                .filter(|value| valid_runtime_value(value))
            {
                values.push(value.clone());
                index += 1;
            }
        }
        index += 1;
    }
    values
}

fn parse_config_value(raw: &str) -> Option<String> {
    if !valid_runtime_value(raw) {
        return None;
    }
    let document = format!("value = {raw}");
    if let Ok(table) = toml::from_str::<toml::Table>(&document) {
        if let Some(value) = table.get("value").and_then(toml::Value::as_str) {
            return valid_runtime_value(value).then(|| value.to_string());
        }
    }
    Some(raw.to_string())
}

#[derive(Clone, Copy)]
enum SessionFormat {
    Codex,
    Claude,
    Copilot,
}

struct CachedRuntime {
    offset: u64,
    settings: AgentRuntimeSettings,
}

static SESSION_PATH_CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
static SESSION_RUNTIME_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedRuntime>>> = OnceLock::new();

/// Read provider-owned session state so interactive model changes are captured,
/// even when they are absent from the process argv.
pub fn from_native_session(
    source: &str,
    agent: &str,
    session_ref: &crate::agent_resume::AgentSessionRef,
) -> Option<AgentRuntimeSettings> {
    if session_ref.kind != crate::agent_resume::AgentSessionRefKind::Id {
        return None;
    }
    let session_id = &session_ref.value;
    if session_id == "."
        || session_id == ".."
        || session_id.contains('/')
        || session_id.contains('\\')
    {
        return None;
    }
    let (path, format) = match (source, agent) {
        ("herdr:codex", "codex") => {
            let root = crate::integration::codex_dir().ok()?.join("sessions");
            (
                cached_session_path("codex", &root, session_id, |root, id| {
                    find_session_file(root, id, false)
                })?,
                SessionFormat::Codex,
            )
        }
        ("herdr:claude", "claude") => {
            let root = crate::integration::claude_dir().ok()?.join("projects");
            (
                cached_session_path("claude", &root, session_id, |root, id| {
                    find_session_file(root, id, true)
                })?,
                SessionFormat::Claude,
            )
        }
        ("herdr:copilot", "copilot") => {
            let path = crate::integration::copilot_dir()
                .ok()?
                .join("session-state")
                .join(session_id)
                .join("events.jsonl");
            path.is_file().then_some((path, SessionFormat::Copilot))?
        }
        _ => return None,
    };

    cached_jsonl_runtime(&path, format)?.validated(source, agent)
}

fn cached_session_path<F>(provider: &str, root: &Path, session_id: &str, find: F) -> Option<PathBuf>
where
    F: FnOnce(&Path, &str) -> Option<PathBuf>,
{
    let key = format!("{provider}\0{}\0{session_id}", root.display());
    let cache = SESSION_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(path) = cache
        .lock()
        .ok()?
        .get(&key)
        .filter(|path| path.is_file())
        .cloned()
    {
        return Some(path);
    }
    let path = find(root, session_id)?;
    cache.lock().ok()?.insert(key, path.clone());
    Some(path)
}

fn find_session_file(root: &Path, session_id: &str, exact_name: bool) -> Option<PathBuf> {
    let expected = format!("{session_id}.jsonl");
    let mut pending = vec![(root.to_path_buf(), 0_u8)];
    while let Some((dir, depth)) = pending.pop() {
        if depth > 5 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                pending.push((entry.path(), depth.saturating_add(1)));
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let matches = if exact_name {
                name == expected
            } else {
                name.ends_with(&expected)
            };
            if matches {
                return Some(entry.path());
            }
        }
    }
    None
}

fn cached_jsonl_runtime(path: &Path, format: SessionFormat) -> Option<AgentRuntimeSettings> {
    let metadata = path.metadata().ok()?;
    let cache = SESSION_RUNTIME_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    let entry = cache
        .entry(path.to_path_buf())
        .or_insert_with(|| CachedRuntime {
            offset: 0,
            settings: AgentRuntimeSettings::default(),
        });
    if metadata.len() < entry.offset {
        entry.offset = 0;
        entry.settings = AgentRuntimeSettings::default();
    }

    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(entry.offset)).ok()?;
    let mut reader = BufReader::new(file);
    scan_complete_lines(&mut reader, entry, format).ok()?;
    (!entry.settings.is_empty()).then(|| entry.settings.clone())
}

fn scan_complete_lines<R: BufRead>(
    reader: &mut R,
    cached: &mut CachedRuntime,
    format: SessionFormat,
) -> std::io::Result<()> {
    let mut line = Vec::new();
    let mut line_overflowed = false;
    let mut consumed = cached.offset;
    let mut line_start = cached.offset;

    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            // Keep an incomplete final line for the next pass.
            cached.offset = line_start;
            return Ok(());
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(buffer.len(), |position| position + 1);
        if !line_overflowed {
            let remaining = MAX_RUNTIME_EVENT_BYTES.saturating_sub(line.len());
            if take <= remaining {
                line.extend_from_slice(&buffer[..take]);
            } else {
                line_overflowed = true;
                line.clear();
            }
        }
        reader.consume(take);
        consumed = consumed.saturating_add(take as u64);

        if newline.is_none() {
            continue;
        }
        if !line_overflowed {
            parse_runtime_line(&line, format, &mut cached.settings);
        }
        cached.offset = consumed;
        line_start = consumed;
        line.clear();
        line_overflowed = false;
    }
}

fn parse_runtime_line(line: &[u8], format: SessionFormat, settings: &mut AgentRuntimeSettings) {
    let candidate = match format {
        SessionFormat::Codex => {
            contains_bytes(line, b"thread_settings_applied")
                || contains_bytes(line, b"turn_context")
        }
        SessionFormat::Claude => {
            (contains_bytes(line, b"assistant") && contains_bytes(line, b"model"))
                || contains_bytes(line, b"init")
                || contains_bytes(line, b"effort")
        }
        SessionFormat::Copilot => {
            contains_bytes(line, b"session.start")
                || contains_bytes(line, b"session.resume")
                || contains_bytes(line, b"session.model_change")
                || contains_bytes(line, b"session.shutdown")
        }
    };
    if !candidate {
        return;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
        return;
    };
    match format {
        SessionFormat::Codex => parse_codex_event(&value, settings),
        SessionFormat::Claude => parse_claude_event(&value, settings),
        SessionFormat::Copilot => parse_copilot_event(&value, settings),
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn parse_codex_event(value: &serde_json::Value, settings: &mut AgentRuntimeSettings) {
    if value.get("type").and_then(serde_json::Value::as_str) == Some("turn_context") {
        let Some(payload) = value.get("payload").and_then(serde_json::Value::as_object) else {
            return;
        };
        set_string_if_present(&mut settings.model, payload, "model");
        set_string_if_present(&mut settings.reasoning_effort, payload, "effort");
        set_string_if_present(&mut settings.reasoning_summary, payload, "summary");
        return;
    }
    if value
        .pointer("/payload/type")
        .and_then(serde_json::Value::as_str)
        != Some("thread_settings_applied")
    {
        return;
    }
    let Some(thread) = value
        .pointer("/payload/thread_settings")
        .and_then(serde_json::Value::as_object)
    else {
        return;
    };
    set_string_if_present(&mut settings.model, thread, "model");
    set_string_if_present(&mut settings.model_provider, thread, "model_provider_id");
    set_string_if_present(&mut settings.reasoning_effort, thread, "reasoning_effort");
    set_string_if_present(&mut settings.service_tier, thread, "service_tier");
}

fn parse_claude_event(value: &serde_json::Value, settings: &mut AgentRuntimeSettings) {
    let event_type = value.get("type").and_then(serde_json::Value::as_str);
    if event_type == Some("assistant") {
        if let Some(model) = value
            .pointer("/message/model")
            .and_then(serde_json::Value::as_str)
            .filter(|model| valid_runtime_value(model))
        {
            settings.model = Some(model.to_string());
        }
    } else if value.get("subtype").and_then(serde_json::Value::as_str) == Some("init") {
        if let Some(object) = value.as_object() {
            set_string_if_present(&mut settings.model, object, "model");
        }
    }
    if let Some(effort) = value
        .pointer("/effort/level")
        .and_then(serde_json::Value::as_str)
        .filter(|effort| valid_runtime_value(effort))
    {
        settings.reasoning_effort = Some(effort.to_string());
    }
}

fn parse_copilot_event(value: &serde_json::Value, settings: &mut AgentRuntimeSettings) {
    let Some(event_type) = value.get("type").and_then(serde_json::Value::as_str) else {
        return;
    };
    let Some(data) = value.get("data").and_then(serde_json::Value::as_object) else {
        return;
    };
    match event_type {
        "session.start" | "session.resume" => {
            set_string_if_present(&mut settings.model, data, "selectedModel");
            set_string_if_present(&mut settings.reasoning_effort, data, "reasoningEffort");
            set_string_if_present(&mut settings.reasoning_summary, data, "reasoningSummary");
            set_string_if_present(&mut settings.verbosity, data, "verbosity");
            set_string_if_present(&mut settings.context_tier, data, "contextTier");
        }
        "session.model_change" => {
            set_string_if_present(&mut settings.model, data, "newModel");
            set_string_if_present(&mut settings.reasoning_effort, data, "reasoningEffort");
            set_string_if_present(&mut settings.reasoning_summary, data, "reasoningSummary");
            set_string_if_present(&mut settings.verbosity, data, "verbosity");
            set_string_if_present(&mut settings.context_tier, data, "contextTier");
        }
        "session.shutdown" => {
            set_string_if_present(&mut settings.model, data, "currentModel");
        }
        _ => {}
    }
}

fn set_string_if_present(
    target: &mut Option<String>,
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    let Some(value) = object.get(key) else {
        return;
    };
    *target = value
        .as_str()
        .filter(|value| valid_runtime_value(value))
        .map(str::to_string);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn argv_capture_is_provider_specific() {
        assert_eq!(
            from_argv(
                "herdr:codex",
                "codex",
                &strings(&[
                    "codex",
                    "--model",
                    "gpt-5.6-sol",
                    "-c",
                    "model_reasoning_effort=\"max\"",
                    "--config=service_tier=\"fast\"",
                    "--yolo",
                ]),
            ),
            Some(AgentRuntimeSettings {
                model: Some("gpt-5.6-sol".into()),
                reasoning_effort: Some("max".into()),
                service_tier: Some("fast".into()),
                ..AgentRuntimeSettings::default()
            })
        );
        assert_eq!(
            from_argv(
                "herdr:copilot",
                "copilot",
                &strings(&[
                    "copilot",
                    "--model=gpt-5.4",
                    "--reasoning-effort=xhigh",
                    "--context",
                    "long_context",
                ]),
            ),
            Some(AgentRuntimeSettings {
                model: Some("gpt-5.4".into()),
                reasoning_effort: Some("xhigh".into()),
                context_tier: Some("long_context".into()),
                ..AgentRuntimeSettings::default()
            })
        );
        assert_eq!(
            from_argv(
                "herdr:opencode",
                "opencode",
                &strings(&[
                    "opencode",
                    "-m",
                    "openai/gpt-5",
                    "--variant=thinking",
                    "--agent",
                    "build",
                ]),
            ),
            Some(AgentRuntimeSettings {
                model: Some("openai/gpt-5".into()),
                variant: Some("thinking".into()),
                agent_profile: Some("build".into()),
                ..AgentRuntimeSettings::default()
            })
        );
        assert_eq!(
            from_argv(
                "herdr:claude",
                "claude",
                &strings(&[
                    "C:\\tools\\claude.cmd",
                    "--model=claude-opus-4-6",
                    "--effort",
                    "high",
                    "--agent",
                    "reviewer",
                ]),
            ),
            Some(AgentRuntimeSettings {
                model: Some("claude-opus-4-6".into()),
                reasoning_effort: Some("high".into()),
                agent_profile: Some("reviewer".into()),
                ..AgentRuntimeSettings::default()
            })
        );
    }

    #[test]
    fn runtime_values_cannot_be_replayed_as_options() {
        assert_eq!(
            from_argv(
                "herdr:claude",
                "claude",
                &strings(&["claude", "--model", "--dangerously-skip-permissions"]),
            ),
            None
        );
        assert_eq!(
            AgentRuntimeSettings {
                model: Some("--yolo".into()),
                ..AgentRuntimeSettings::default()
            }
            .resume_args("herdr:codex", "codex"),
            Vec::<String>::new()
        );
        assert_eq!(
            from_argv(
                "herdr:codex",
                "codex",
                &strings(&["codex", "--", "--model", "prompt-text"]),
            ),
            None
        );
        assert_eq!(
            from_native_session(
                "herdr:copilot",
                "copilot",
                &crate::agent_resume::AgentSessionRef::id("../outside").unwrap(),
            ),
            None
        );
    }

    #[test]
    fn resume_args_reapply_only_supported_settings() {
        let codex = AgentRuntimeSettings {
            model: Some("gpt-5.6-terra".into()),
            model_provider: Some("openai".into()),
            reasoning_effort: Some("medium".into()),
            service_tier: Some("fast".into()),
            context_tier: Some("ignored".into()),
            ..AgentRuntimeSettings::default()
        };
        assert_eq!(
            codex.resume_args("herdr:codex", "codex"),
            strings(&[
                "--model",
                "gpt-5.6-terra",
                "--config",
                "model_provider=\"openai\"",
                "--config",
                "model_reasoning_effort=\"medium\"",
                "--config",
                "service_tier=\"fast\"",
            ])
        );
        assert_eq!(
            AgentRuntimeSettings {
                model: Some("claude-opus-4-6".into()),
                reasoning_effort: Some("max".into()),
                agent_profile: Some("reviewer".into()),
                ..AgentRuntimeSettings::default()
            }
            .resume_args("herdr:claude", "claude"),
            strings(&[
                "--model",
                "claude-opus-4-6",
                "--effort",
                "max",
                "--agent",
                "reviewer",
            ])
        );
        assert_eq!(
            AgentRuntimeSettings {
                model: Some("anthropic/claude-opus-4-6".into()),
                variant: Some("high".into()),
                agent_profile: Some("build".into()),
                ..AgentRuntimeSettings::default()
            }
            .resume_args("herdr:opencode", "opencode"),
            strings(&["--model", "anthropic/claude-opus-4-6", "--agent", "build",])
        );
    }

    #[test]
    fn parsers_follow_interactive_model_changes() {
        let dir = std::env::temp_dir().join(format!(
            "herdr-agent-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let codex_path = dir.join("codex.jsonl");
        std::fs::write(
            &codex_path,
            concat!(
                "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-old\",\"effort\":\"low\",\"summary\":\"concise\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\",\"thread_settings\":{\"model\":\"gpt-new\",\"model_provider_id\":\"openai\",\"reasoning_effort\":\"high\",\"service_tier\":\"fast\"}}}\n",
            ),
        )
        .unwrap();
        assert_eq!(
            cached_jsonl_runtime(&codex_path, SessionFormat::Codex),
            Some(AgentRuntimeSettings {
                model: Some("gpt-new".into()),
                model_provider: Some("openai".into()),
                reasoning_effort: Some("high".into()),
                reasoning_summary: Some("concise".into()),
                service_tier: Some("fast".into()),
                ..AgentRuntimeSettings::default()
            })
        );

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&codex_path)
            .unwrap();
        writeln!(
            file,
            "{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"gpt-latest\",\"effort\":\"max\"}}}}"
        )
        .unwrap();
        let latest = cached_jsonl_runtime(&codex_path, SessionFormat::Codex).unwrap();
        assert_eq!(latest.model.as_deref(), Some("gpt-latest"));
        assert_eq!(latest.reasoning_effort.as_deref(), Some("max"));
        assert_eq!(latest.reasoning_summary.as_deref(), Some("concise"));
        assert_eq!(latest.service_tier.as_deref(), Some("fast"));

        let copilot_path = dir.join("copilot.jsonl");
        std::fs::write(
            &copilot_path,
            concat!(
                "{\"type\":\"session.start\",\"data\":{\"selectedModel\":\"claude-sonnet-4.6\",\"reasoningEffort\":\"high\",\"contextTier\":\"default\"}}\n",
                "{\"type\":\"session.model_change\",\"data\":{\"newModel\":\"gpt-5.4\",\"reasoningEffort\":\"xhigh\",\"reasoningSummary\":\"detailed\",\"verbosity\":\"low\",\"contextTier\":\"long_context\"}}\n",
            ),
        )
        .unwrap();
        assert_eq!(
            cached_jsonl_runtime(&copilot_path, SessionFormat::Copilot),
            Some(AgentRuntimeSettings {
                model: Some("gpt-5.4".into()),
                reasoning_effort: Some("xhigh".into()),
                reasoning_summary: Some("detailed".into()),
                verbosity: Some("low".into()),
                context_tier: Some("long_context".into()),
                ..AgentRuntimeSettings::default()
            })
        );

        let claude_path = dir.join("claude.jsonl");
        std::fs::write(
            &claude_path,
            concat!(
                "{\"type\": \"assistant\", \"message\": {\"model\": \"claude-sonnet-4-6\"}}\n",
                "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-4-6\"},\"effort\":{\"level\":\"high\"}}\n",
            ),
        )
        .unwrap();
        assert_eq!(
            cached_jsonl_runtime(&claude_path, SessionFormat::Claude),
            Some(AgentRuntimeSettings {
                model: Some("claude-opus-4-6".into()),
                reasoning_effort: Some("high".into()),
                ..AgentRuntimeSettings::default()
            })
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
