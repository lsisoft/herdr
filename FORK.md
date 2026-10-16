# lsisoft Herdr fork

The maintained branch is [custom/herdr](https://github.com/lsisoft/herdr/tree/custom/herdr).
`master` tracks upstream. The former `feature/topbar-agent-status` branch is
deprecated; its recovered work has been adapted to the current client. Old
upstream topic branches are archived locally instead of maintained on this fork.

## Passive agent status on tabs

Choose **Settings → tabs**, or add:

```toml
[ui]
tab_agent_status = "attention" # off | attention | all
status_indicators = "symbols" # dots | symbols
hide_tab_bar_when_single_tab = false
```

The default is off. Attention shows blocked and unseen done; all also shows
working and idle. Unknown has no marker. Markers are static, use the shared
theme colors and indicator style, and appear before each label whether the
sidebar is open, compact, or hidden. They work with top and bottom tab rows.
The selected label retains its highlight while the marker has a neutral
background. A reserved gutter prevents labels shifting as states change.
Settings and file edits apply live.

The existing aggregate priority remains blocked, unseen done, working, idle,
unknown. Completion acknowledgement remains per client. This adds no popup,
sound, spinner, pane detection, or protocol field.

This combines the common requests in
[discussion #1429](https://github.com/herdrdev/herdr/discussions/1429),
including the later request for attention-only signals. The July prototype
and [errogaht's implementation](https://github.com/errogaht/herdr/commit/7d2367b177c4ec156130d5528649e2e6781dc4ab)
provided prior behaviour and test ideas. Rendering now belongs to
`src/client/shell/tabs.rs`, which consumes existing tab status from the server.

## Earlier fork behaviour retained

- Preserve allow-listed native resume permissions and observed model/runtime
  settings. Explicit agent-reported resume commands remain authoritative.
  Missing observations remain unknown; no credentials, prompts, environment,
  or arbitrary provider configuration are saved in the profile.
- Use a manual root-pane label as an automatic tab's display name. Explicit tab
  names win. Pane, tab, sidebar, navigator, mobile, and API surfaces agree on
  names; manual pane labels take priority over generated titles.
- Allow explicitly requested control from agents outside Herdr. Select the
  server/session first and use explicit discovered IDs. Caller-relative
  commands remain limited to a verified managed caller. See the
  [fork skill](skills/herdr/SKILL.md).

The recovery tree's terminal restoration fix is already covered by upstream's
single-claim terminal guard and fallible restoration. Its older layout and
settings code has been replaced by the current client implementation.

Claude and OpenCode integrations carry fork migration versions 12 and 14.
Refresh integrations from Settings after installing this build.

## Build

Use the Rust version in `rust-toolchain.toml`, Zig 0.16.0, and the repository's
`just` recipes. Build with `just build`. This branch is a custom build and
must be installed from the fork; `herdr update` installs upstream releases.

## Verification (2026-10-16)

Rebased on upstream `143bf5fd`. GPT-6 Astra reviewed the changes; all actionable
findings were fixed and regression-tested. Native `just ci` passed: 3,950 Rust
tests plus maintenance, architecture, and integration asset checks.
Documentation contracts, config reference, and translation parity passed.

The release-mode tab renderer measured about 0.8 µs with one populated pane and
4.6 µs with fifteen in all-status mode (fixed geometry, 10,000 iterations).
The broader optional graphics benchmark could not finish its unrelated CoW
source-retention scenario on this host. Windows cross-lint was unavailable
because the Windows SDK is not installed. Newer OpenCode TUI payloads were
checked against upstream schemas and mocked integration tests, not a live
newer OpenCode installation.
