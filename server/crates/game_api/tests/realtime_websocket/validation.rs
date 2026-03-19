use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_skill_tier_skips_and_accepts_the_next_valid_pick() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;
    let _ = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;

    send_command(&mut alice, ClientControlCommand::CreateGameLobby, 2).await;
    let created_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbyCreated { .. })
    })
    .await;
    let lobby_id = created_events
        .into_iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(lobby_id),
            _ => None,
        })
        .expect("create flow should include GameLobbyCreated");
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut alice,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        },
        3,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamB,
        },
        3,
    )
    .await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut bob).await;
    let _ = recv_event(&mut bob).await;

    send_command(
        &mut alice,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    let _ = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;

    send_command(
        &mut alice,
        ClientControlCommand::ChooseSkill {
            tree: game_domain::SkillTree::Mage,
            tier: 5,
        },
        5,
    )
    .await;
    let alice_invalid_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::Error { .. })
    })
    .await;
    assert!(alice_invalid_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "skill progression for Mage expected tier 1 but received tier 5"
    )));

    send_command(
        &mut alice,
        ClientControlCommand::ChooseSkill {
            tree: game_domain::SkillTree::Mage,
            tier: 1,
        },
        6,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::ChooseSkill {
            tree: game_domain::SkillTree::Rogue,
            tier: 1,
        },
        5,
    )
    .await;

    let alice_events = recv_events_until(&mut alice, 24, |event| {
        matches!(event, ServerControlEvent::PreCombatStarted { .. })
    })
    .await;
    let bob_events = recv_events_until(&mut bob, 24, |event| {
        matches!(event, ServerControlEvent::PreCombatStarted { .. })
    })
    .await;
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::SkillChosen {
            tree: game_domain::SkillTree::Mage,
            tier: 1,
            ..
        }
    )));
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::SkillChosen {
            tree: game_domain::SkillTree::Rogue,
            tier: 1,
            ..
        }
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_input_frames_before_combat() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;
    let _ = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;

    send_command(&mut alice, ClientControlCommand::CreateGameLobby, 2).await;
    let created_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbyCreated { .. })
    })
    .await;
    let lobby_id = created_events
        .into_iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(lobby_id),
            _ => None,
        })
        .expect("create flow should include GameLobbyCreated");
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut alice,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        },
        3,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamB,
        },
        3,
    )
    .await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut bob).await;
    let _ = recv_event(&mut bob).await;

    send_command(
        &mut alice,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    let _ = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;

    send_input(
        &mut alice,
        ValidatedInputFrame::new(1, 0, 0, 0, 0, BUTTON_PRIMARY, 0)
            .expect("primary attack frame should be valid"),
        1,
        1,
    )
    .await;

    let alice_error_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::Error { .. })
    })
    .await;
    assert!(alice_error_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "input frames are only accepted during combat"
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_rejects_live_input_axis_and_locked_skill_slot_cheats() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    let mut bob = connect_socket(&bootstrap_signal_url(&base_url).await).await;

    connect_player(&mut alice, "Alice").await;
    connect_player(&mut bob, "Bob").await;
    let _ = recv_events_until(&mut alice, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;

    send_command(&mut alice, ClientControlCommand::CreateGameLobby, 2).await;
    let created_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbyCreated { .. })
    })
    .await;
    let lobby_id = created_events
        .into_iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(lobby_id),
            _ => None,
        })
        .expect("create flow should include GameLobbyCreated");
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut bob,
        ClientControlCommand::JoinGameLobby { lobby_id },
        2,
    )
    .await;
    let _ = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 4, |event| {
        matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
    })
    .await;

    send_command(
        &mut alice,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        },
        3,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SelectTeam {
            team: TeamSide::TeamB,
        },
        3,
    )
    .await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut alice).await;
    let _ = recv_event(&mut bob).await;
    let _ = recv_event(&mut bob).await;

    send_command(
        &mut alice,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        },
        4,
    )
    .await;
    let _ = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;

    send_command(
        &mut alice,
        ClientControlCommand::ChooseSkill {
            tree: SkillTree::Mage,
            tier: 1,
        },
        5,
    )
    .await;
    send_command(
        &mut bob,
        ClientControlCommand::ChooseSkill {
            tree: SkillTree::Rogue,
            tier: 1,
        },
        5,
    )
    .await;
    let _ = recv_events_until(&mut alice, 24, |event| {
        matches!(event, ServerControlEvent::PreCombatStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut bob, 24, |event| {
        matches!(event, ServerControlEvent::PreCombatStarted { .. })
    })
    .await;
    let _ = recv_events_until(&mut alice, 8, |event| {
        matches!(event, ServerControlEvent::CombatStarted)
    })
    .await;
    let _ = recv_events_until(&mut bob, 8, |event| {
        matches!(event, ServerControlEvent::CombatStarted)
    })
    .await;
    let _ = drain_pending_events(&mut alice, Duration::from_millis(30), 16).await;
    let _ = drain_pending_events(&mut bob, Duration::from_millis(30), 16).await;

    send_input(
        &mut alice,
        ValidatedInputFrame::new(1, 0, 0, 0, 0, BUTTON_CAST, 5)
            .expect("locked-slot cast frame should encode"),
        1,
        1,
    )
    .await;
    let locked_slot_events =
        recv_events_until_within(&mut alice, Duration::from_millis(500), 16, |event| {
            matches!(event, ServerControlEvent::Error { .. })
        })
        .await;
    assert!(locked_slot_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "skill slot 5 is not unlocked for round 1"
    )));

    let _ = drain_pending_events(&mut alice, Duration::from_millis(30), 16).await;

    send_input(
        &mut alice,
        ValidatedInputFrame::new(2, 2, 0, 0, 0, 0, 0).expect("axis-cheat frame should encode"),
        2,
        2,
    )
    .await;
    let axis_cheat_events =
        recv_events_until_within(&mut alice, Duration::from_millis(500), 16, |event| {
            matches!(event, ServerControlEvent::Error { .. })
        })
        .await;
    assert!(axis_cheat_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "move_horizontal_q=2 is outside the allowed range -1..=1"
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}
