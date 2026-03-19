use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn webrtc_transport_connects_and_streams_control_plus_snapshot_events() {
    let (server, base_url) = start_server_fast().await;
    let mut alice = WebRtcClient::connect(&base_url, "Alice").await;
    let mut bob = WebRtcClient::connect(&base_url, "Bob").await;

    let alice_connect_events: Vec<ServerControlEvent> = alice
        .recv_events_until(3, |event| {
            matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
        })
        .await;
    let bob_connect_events: Vec<ServerControlEvent> = bob
        .recv_events_until(3, |event| {
            matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
        })
        .await;
    assert!(alice_connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));
    assert!(bob_connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));

    alice
        .send_command(ClientControlCommand::CreateGameLobby)
        .await;
    let created_events: Vec<ServerControlEvent> = alice
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbyCreated { .. })
        })
        .await;
    let lobby_id = created_events
        .iter()
        .find_map(|event| match event {
            ServerControlEvent::GameLobbyCreated { lobby_id } => Some(*lobby_id),
            _ => None,
        })
        .expect("create flow should include GameLobbyCreated");
    let _ = alice
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
        })
        .await;

    bob.send_command(ClientControlCommand::JoinGameLobby { lobby_id })
        .await;
    let _ = alice
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
        })
        .await;
    let _ = bob
        .recv_events_until(4, |event| {
            matches!(event, ServerControlEvent::GameLobbySnapshot { .. })
        })
        .await;

    alice
        .send_command(ClientControlCommand::SelectTeam {
            team: TeamSide::TeamA,
        })
        .await;
    bob.send_command(ClientControlCommand::SelectTeam {
        team: TeamSide::TeamB,
    })
    .await;
    let _ = alice.recv_event().await;
    let _ = alice.recv_event().await;
    let _ = bob.recv_event().await;
    let _ = bob.recv_event().await;

    alice
        .send_command(ClientControlCommand::SetReady {
            ready: ReadyState::Ready,
        })
        .await;
    bob.send_command(ClientControlCommand::SetReady {
        ready: ReadyState::Ready,
    })
    .await;

    let alice_launch_events: Vec<ServerControlEvent> = alice
        .recv_events_until(24, |event| {
            matches!(event, ServerControlEvent::ArenaStateSnapshot { .. })
        })
        .await;
    let bob_launch_events: Vec<ServerControlEvent> = bob
        .recv_events_until(24, |event| {
            matches!(event, ServerControlEvent::ArenaStateSnapshot { .. })
        })
        .await;
    assert!(alice_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::MatchStarted { .. })));
    assert!(bob_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::MatchStarted { .. })));
    assert!(alice_launch_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaStateSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Alice")
                && !snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
                && !snapshot.obstacles.is_empty()
                && !snapshot.visible_tiles.is_empty()
    )));
    assert!(bob_launch_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaStateSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
                && !snapshot.players.iter().any(|player| player.player_name.as_str() == "Alice")
                && !snapshot.obstacles.is_empty()
                && !snapshot.visible_tiles.is_empty()
    )));

    alice
        .send_command(ClientControlCommand::ChooseSkill {
            tree: SkillTree::Rogue,
            tier: 1,
        })
        .await;
    bob.send_command(ClientControlCommand::ChooseSkill {
        tree: SkillTree::Warrior,
        tier: 1,
    })
    .await;
    let _ = alice
        .recv_events_until(10, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
    let _ = bob
        .recv_events_until(10, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
    let _ = alice
        .recv_events_until(8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;
    let _ = bob
        .recv_events_until(8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;

    alice.send_input(slot_one_cast_input(101), 101).await;
    let alice_combat_events = alice
        .recv_events_until(24, |event| {
            matches!(
                event,
                ServerControlEvent::ArenaDeltaSnapshot { snapshot }
                    if snapshot.players.iter().any(|player| player.mana < player.max_mana)
            )
        })
        .await;
    let bob_combat_events = bob
        .recv_events_until(24, |event| {
            matches!(
                event,
                ServerControlEvent::ArenaDeltaSnapshot { snapshot }
                    if snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
            )
        })
        .await;
    assert!(alice_combat_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaDeltaSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.mana < player.max_mana)
    )));
    assert!(bob_combat_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaDeltaSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Bob")
    )));

    alice.close().await;
    bob.close().await;
    server.shutdown().await;
}
