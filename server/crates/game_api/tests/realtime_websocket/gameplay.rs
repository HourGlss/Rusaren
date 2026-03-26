use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_adapter_finishes_a_full_match_loop_via_live_input_frames() {
    let content_root = websocket_gameplay_content_root();
    let (server, base_url) = start_server_fast_with_content_root(content_root).await;
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

    let alice_launch_events = recv_events_until(&mut alice, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    let bob_launch_events = recv_events_until(&mut bob, 16, |event| {
        matches!(event, ServerControlEvent::MatchStarted { .. })
    })
    .await;
    assert!(alice_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));
    assert!(bob_launch_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::LaunchCountdownStarted { .. })));
    let alice_snapshot_events = recv_events_until(&mut alice, 4, |event| {
        matches!(event, ServerControlEvent::ArenaStateSnapshot { .. })
    })
    .await;
    assert!(alice_snapshot_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaStateSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.player_name.as_str() == "Alice")
                && !snapshot.visible_tiles.is_empty()
                && snapshot.projectiles.is_empty()
    )));

    for round in 1..=5 {
        send_command(
            &mut alice,
            ClientControlCommand::ChooseSkill {
                tree: game_domain::SkillTree::Mage,
                tier: round,
            },
            4 + u32::from(round),
        )
        .await;
        send_command(
            &mut bob,
            ClientControlCommand::ChooseSkill {
                tree: game_domain::SkillTree::Rogue,
                tier: round,
            },
            4 + u32::from(round),
        )
        .await;

        let alice_skill_events = recv_events_until(&mut alice, 24, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
        let bob_skill_events = recv_events_until(&mut bob, 24, |event| {
            matches!(event, ServerControlEvent::PreCombatStarted { .. })
        })
        .await;
        assert!(alice_skill_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::SkillChosen { .. })));
        assert!(bob_skill_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::SkillChosen { .. })));

        let _ = recv_events_until(&mut alice, 8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;
        let _ = recv_events_until(&mut bob, 8, |event| {
            matches!(event, ServerControlEvent::CombatStarted)
        })
        .await;

        if round < 5 {
            let (alice_round_events, bob_round_events) =
                cast_until_round_won(&mut alice, &mut bob, round).await;
            assert!(alice_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::ArenaEffectBatch { effects }
                    if effects.iter().any(|effect| effect.slot == 1)
            )));
            assert!(alice_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team: TeamSide::TeamA,
                    score_a,
                    score_b,
                } if won.get() == round && *score_a == round && *score_b == 0
            )));
            assert!(bob_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team: TeamSide::TeamA,
                    score_a,
                    score_b,
                } if won.get() == round && *score_a == round && *score_b == 0
            )));
        } else {
            let (mut alice_match_events, mut bob_match_events) =
                cast_until_round_won(&mut alice, &mut bob, round).await;
            alice_match_events.extend(
                recv_events_until(&mut alice, 8, |event| {
                    matches!(event, ServerControlEvent::MatchEnded { .. })
                })
                .await,
            );
            bob_match_events.extend(
                recv_events_until(&mut bob, 8, |event| {
                    matches!(event, ServerControlEvent::MatchEnded { .. })
                })
                .await,
            );
            assert!(alice_match_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team: TeamSide::TeamA,
                    score_a,
                    score_b,
                } if won.get() == round && *score_a == 5 && *score_b == 0
            )));
            assert!(alice_match_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::MatchEnded {
                    outcome: MatchOutcome::TeamAWin,
                    score_a: 5,
                    score_b: 0,
                    ..
                }
            )));
            assert!(bob_match_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::MatchEnded {
                    outcome: MatchOutcome::TeamAWin,
                    score_a: 5,
                    score_b: 0,
                    ..
                }
            )));
        }
    }

    send_command(&mut alice, ClientControlCommand::QuitToCentralLobby, 10).await;
    send_command(&mut bob, ClientControlCommand::QuitToCentralLobby, 10).await;
    let alice_return_events = recv_events_until(&mut alice, 8, |event| {
        matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })
    })
    .await;
    let bob_return_events = recv_events_until(&mut bob, 8, |event| {
        matches!(event, ServerControlEvent::ReturnedToCentralLobby { .. })
    })
    .await;
    assert!(alice_return_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 1 && record.losses == 0 && record.no_contests == 0
    )));
    assert!(bob_return_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 0 && record.losses == 1 && record.no_contests == 0
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}
