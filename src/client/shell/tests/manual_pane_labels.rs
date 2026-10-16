use super::*;

fn named_root_snapshot() -> ClientShellSnapshot {
    let mut projected = snapshot();
    projected.tabs[0].label = "reviewer".into();
    projected.panes[0].label = Some("reviewer".into());
    projected.agents.push(ClientShellAgent {
        pane_id: "pane_1".into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some("pi".into()),
        display_agent: None,
        agent: Some("pi".into()),
        title: Some("generated title".into()),
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Working,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: true,
    });
    projected.tabs[0].agent_status = AgentStatus::Working;
    projected
}

#[test]
fn tab_bar_renders_inherited_names_with_shared_status_indicators() {
    use crate::config::StatusIndicatorStyle;

    let mut projected = named_root_snapshot();
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.tab_agent_status = crate::config::TabAgentStatusConfig::All;
    let cases = [
        (AgentStatus::Working, "◐", "●", config.palette.yellow),
        (AgentStatus::Blocked, "×", "●", config.palette.red),
        (AgentStatus::Done, "✓", "●", config.palette.teal),
        (AgentStatus::Idle, "○", "○", config.palette.green),
        (AgentStatus::Unknown, " ", " ", config.palette.overlay0),
    ];
    for (status, symbol, dot, color) in cases {
        for (indicators, expected) in [
            (StatusIndicatorStyle::Symbols, symbol),
            (StatusIndicatorStyle::Dots, dot),
        ] {
            for focused in [false, true] {
                projected.tabs[0].agent_status = status;
                projected.tabs[0].focused = focused;
                config.status_indicators = indicators;
                let area = Rect::new(0, 0, 40, 1);
                let mut buffer = Buffer::empty(area);
                let mut scroll = 0;
                let mut reveal = true;
                let mut hits = ShellHitMap::default();
                render::render_tab_bar(
                    &mut buffer,
                    area,
                    &projected,
                    &config,
                    false,
                    &mut scroll,
                    &mut reveal,
                    None,
                    &mut hits,
                );

                let (rect, tab_id) = &hits.tabs[0];
                assert_eq!(tab_id, "tab_1");
                assert_eq!(rect.width, 14);
                let icon = &buffer[(rect.x + 1, rect.y)];
                assert_eq!(icon.symbol(), expected);
                if status != AgentStatus::Unknown {
                    assert_eq!(icon.fg, color);
                }
                assert_eq!(icon.bg, config.palette.surface0);
                assert_eq!(
                    buffer[(rect.x + 3, rect.y)].bg,
                    if focused {
                        config.palette.accent
                    } else {
                        config.palette.surface0
                    }
                );
                let text = buffer
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                assert!(text.contains("reviewer"), "{text:?}");
            }
        }
    }
}

#[test]
fn inherited_tab_name_changes_reflow_hits_and_allow_narrow_unicode_tabs() {
    let mut projected = named_root_snapshot();
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.tab_agent_status = crate::config::TabAgentStatusConfig::All;
    config.mouse_capture = false;
    for (label, zoomed, desired_width) in [("reviewer", false, 14), ("提交", true, 12)] {
        projected.tabs[0].label = label.into();
        projected.tabs[0].zoomed = zoomed;
        for width in [1, 2, 3, 40] {
            let area = Rect::new(0, 0, width, 1);
            let mut buffer = Buffer::empty(area);
            let mut scroll = 0;
            let mut reveal = true;
            let mut hits = ShellHitMap::default();
            render::render_tab_bar(
                &mut buffer,
                area,
                &projected,
                &config,
                false,
                &mut scroll,
                &mut reveal,
                None,
                &mut hits,
            );
            let (rect, tab_id) = &hits.tabs[0];
            assert_eq!(tab_id, "tab_1");
            assert_eq!(rect.width, width.min(desired_width));
            assert!(rect.right() <= area.right());
            if width == 40 {
                // Wide glyphs occupy a symbol cell followed by a continuation cell.
                for (offset, character) in label.char_indices() {
                    let x = rect.x + 3 + render::display_width(&label[..offset]);
                    assert_eq!(buffer[(x, rect.y)].symbol(), character.to_string());
                }
                let marker_x = rect.x + 4 + render::display_width(label);
                assert_eq!(
                    buffer[(marker_x, rect.y)].symbol(),
                    if zoomed { "Z" } else { " " }
                );
            }
        }
    }

    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(named_root_snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("initial tab label");
    let first_width = state.hits.tabs[0].0.width;
    let mut renamed = state.snapshot.as_deref().unwrap().clone();
    renamed.revision += 1;
    renamed.tabs[0].label = "reviewer with a longer name".into();
    renamed.panes[0].label = Some(renamed.tabs[0].label.clone());
    let mut renamed_surface = surface();
    renamed_surface.projection_revision = renamed.revision;
    state.set_snapshot(Box::new(renamed));
    state.set_pane_surface(renamed_surface);
    let frame = state.compose(106, 20).expect("renamed tab label");
    assert!(state.hits.tabs[0].0.width > first_width);
    assert_eq!(state.hits.tabs[0].1, "tab_1");
    assert!(frame_rows(&frame)[0].contains("reviewer with a longer name"));
}

#[test]
fn attention_tabs_keep_geometry_across_status_changes_and_sidebar_modes() {
    use crate::config::{TabAgentStatusConfig, TabBarPositionConfig};
    for position in [TabBarPositionConfig::Top, TabBarPositionConfig::Bottom] {
        for collapsed in [false, true] {
            let mut config = ClientShellConfig::from_config(&Config::default());
            config.tab_bar_position = position;
            config.sidebar_collapsed_mode = crate::config::SidebarCollapsedModeConfig::Hidden;
            config.tab_agent_status = TabAgentStatusConfig::Attention;
            let area = config.layout(120, 24, collapsed, 1, 26).tab_bar;
            let mut projected = named_root_snapshot();
            let mut previous_rect = None;
            for status in [
                AgentStatus::Blocked,
                AgentStatus::Done,
                AgentStatus::Working,
                AgentStatus::Idle,
                AgentStatus::Unknown,
            ] {
                projected.tabs[0].agent_status = status;
                let mut buffer = Buffer::empty(Rect::new(0, 0, 120, 24));
                let mut hits = ShellHitMap::default();
                render::render_tab_bar(
                    &mut buffer,
                    area,
                    &projected,
                    &config,
                    false,
                    &mut 0,
                    &mut true,
                    None,
                    &mut hits,
                );
                let rect = hits.tabs[0].0;
                assert_eq!(
                    buffer[(rect.x + 1, rect.y)].symbol(),
                    if matches!(status, AgentStatus::Blocked | AgentStatus::Done) {
                        "●"
                    } else {
                        " "
                    }
                );
                if let Some(previous) = previous_rect {
                    assert_eq!(rect, previous);
                }
                previous_rect = Some(rect);
                assert_eq!(buffer[(rect.x + 3, rect.y)].symbol(), "r");
                // Rendering never acknowledges a completion or changes its source.
                assert_eq!(projected.tabs[0].agent_status, status);
            }
            config.hide_tab_bar_when_single_tab = true;
            assert_eq!(config.layout(120, 24, collapsed, 1, 26).tab_bar.height, 0);
        }
    }
}

#[test]
fn disabled_tab_status_matches_upstream_label_width_and_padding() {
    let projected = named_root_snapshot();
    let config = ClientShellConfig::from_config(&Config::default());
    let area = Rect::new(0, 0, 40, 1);
    let mut buffer = Buffer::empty(area);
    let mut hits = ShellHitMap::default();
    render::render_tab_bar(
        &mut buffer,
        area,
        &projected,
        &config,
        false,
        &mut 0,
        &mut true,
        None,
        &mut hits,
    );
    assert_eq!(hits.tabs[0].0.width, 12);
    assert_eq!(buffer[(2, 0)].symbol(), "r");
    assert!(!buffer.content.iter().any(|cell| cell.symbol() == "●"));
}

#[test]
#[ignore = "manual render scaling profile"]
fn tab_status_render_scale_profile() {
    use crate::config::TabAgentStatusConfig;
    for count in [1, 15] {
        let mut projected = named_root_snapshot();
        for index in 1..count {
            let mut tab = projected.tabs[0].clone();
            tab.tab_id = format!("tab_{}", index + 1);
            tab.label = format!("agent_{}", index + 1);
            tab.focused = false;
            projected.tabs.push(tab);
            let mut agent = projected.agents[0].clone();
            agent.pane_id = format!("pane_{}", index + 1);
            agent.tab_id = format!("tab_{}", index + 1);
            projected.agents.push(agent);
            let mut pane = projected.panes[0].clone();
            pane.pane_id = format!("pane_{}", index + 1);
            pane.tab_id = format!("tab_{}", index + 1);
            projected.panes.push(pane);
        }
        for mode in [
            TabAgentStatusConfig::Off,
            TabAgentStatusConfig::Attention,
            TabAgentStatusConfig::All,
        ] {
            let mut config = ClientShellConfig::from_config(&Config::default());
            config.tab_agent_status = mode;
            let area = Rect::new(0, 0, 120, 1);
            let mut buffer = Buffer::empty(area);
            let mut hits = ShellHitMap::default();
            let started = std::time::Instant::now();
            for _ in 0..10_000 {
                hits.tabs.clear();
                render::render_tab_bar(
                    &mut buffer,
                    area,
                    std::hint::black_box(&projected),
                    &config,
                    false,
                    &mut 0,
                    &mut true,
                    None,
                    &mut hits,
                );
                std::hint::black_box(&buffer);
            }
            println!(
                "tab_status_render_scale_profile: panes={count} mode={} ns/render={}",
                mode.as_str(),
                started.elapsed().as_nanos() / 10_000
            );
        }
    }
}

#[test]
fn default_sidebar_prefers_manual_names_and_elides_repeated_tab_labels() {
    let config = ClientShellConfig::from_config(&Config::default());
    for multiple_tabs in [false, true] {
        let mut projected = named_root_snapshot();
        if multiple_tabs {
            let mut second = projected.tabs[0].clone();
            second.tab_id = "tab_2".into();
            second.number = 2;
            second.label = "2".into();
            second.focused = false;
            projected.tabs.push(second);
        }
        let area = Rect::new(0, 0, 40, 12);
        let mut buffer = Buffer::empty(area);
        let mut scroll = 0;
        let mut hits = ShellHitMap::default();
        super::super::agent_sidebar::render_agent_panel(
            &mut buffer,
            area,
            &projected,
            &config,
            &mut scroll,
            &mut hits,
        );
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(text.matches("reviewer").count(), 1, "{text:?}");
        assert!(!text.contains("generated title"), "{text:?}");
        assert!(text.contains("pi"), "{text:?}");
        assert_eq!(hits.agents.len(), 1);
        assert_eq!(hits.agents[0].1, "pane_1");
    }
}

#[test]
fn inherited_tab_name_search_keeps_the_manual_pane_target() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(named_root_snapshot()));
    state.open_navigator_overlay();
    let Some(ClientShellOverlay::Navigator(navigator)) = state.overlay.as_mut() else {
        panic!("navigator");
    };
    navigator.query = "reviewer".into();
    let rows =
        render::client_navigator_rows(&state.endpoints, &state.active_endpoint_id, navigator);
    let panes = rows
        .iter()
        .filter(|row| matches!(row.target, ClientNavigatorTarget::Pane { .. }))
        .collect::<Vec<_>>();
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].label, "reviewer");
    assert_eq!(
        super::super::aggregate_navigation::selected_navigator_target(&rows, navigator),
        Some(ClientNavigatorTarget::Pane {
            endpoint_id: state.active_endpoint_id.clone(),
            pane_id: "pane_1".into(),
        })
    );
}

#[test]
fn unchanged_inherited_tab_name_remains_automatic_after_rename_dialog() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(named_root_snapshot()));
    state.open_rename_tab_overlay();
    let Some(ClientShellOverlay::Rename(rename)) = state.overlay.as_ref() else {
        panic!("rename tab");
    };
    assert_eq!(rename.input.as_str(), "reviewer");
    let unchanged = state.handle_input_bytes(b"\r");
    assert!(unchanged.actions.is_empty());
    assert!(state.overlay.is_none());

    state.open_rename_tab_overlay();
    let Some(ClientShellOverlay::Rename(rename)) = state.overlay.as_mut() else {
        panic!("rename tab");
    };
    rename.input = TextEditor::from("explicit tab");
    let renamed = state.handle_input_bytes(b"\r");
    assert!(renamed.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(
                &request.method,
                crate::api::schema::Method::TabRename(params)
                    if params.tab_id == "tab_1" && params.label == "explicit tab"
            )
    )));
}

#[test]
fn mobile_header_and_switcher_use_inherited_tab_names() {
    let mut projected = named_root_snapshot();
    projected.agents.clear();
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    let header = state.compose(44, 24).expect("mobile header");
    assert!(frame_rows(&header).join("\n").contains("tab reviewer"));

    state.mode = ClientShellMode::Navigate;
    let switcher = state.compose(44, 24).expect("mobile switcher");
    assert!(frame_rows(&switcher).join("\n").contains("tab reviewer"));
    assert!(state.hits.mobile_targets.iter().any(|(_, target)| matches!(
        target,
        ClientMobileTarget::Tab { tab_id, .. } if tab_id == "tab_1"
    )));
}
