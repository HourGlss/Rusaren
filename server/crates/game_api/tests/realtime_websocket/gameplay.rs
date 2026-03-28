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

    let mut final_outcome = None;

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
            let alice_round = alice_round_events.iter().find_map(|event| match event {
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team,
                    score_a,
                    score_b,
                } if won.get() == round => Some((*winning_team, *score_a, *score_b)),
                _ => None,
            });
            let bob_round = bob_round_events.iter().find_map(|event| match event {
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team,
                    score_a,
                    score_b,
                } if won.get() == round => Some((*winning_team, *score_a, *score_b)),
                _ => None,
            });
            assert!(alice_round_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::ArenaEffectBatch { effects }
                    if effects.iter().any(|effect| effect.slot == 1)
            )));
            assert_eq!(
                alice_round, bob_round,
                "both players should observe the same round result for round {round}"
            );
            let (_winning_team, score_a, score_b) =
                alice_round.expect("alice should observe a RoundWon event for the current round");
            assert_eq!(
                u16::from(score_a) + u16::from(score_b),
                u16::from(round),
                "round scores should advance cumulatively by one round at a time"
            );
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
            let alice_round = alice_match_events.iter().find_map(|event| match event {
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team,
                    score_a,
                    score_b,
                } if won.get() == round => Some((*winning_team, *score_a, *score_b)),
                _ => None,
            });
            let bob_round = bob_match_events.iter().find_map(|event| match event {
                ServerControlEvent::RoundWon {
                    round: won,
                    winning_team,
                    score_a,
                    score_b,
                } if won.get() == round => Some((*winning_team, *score_a, *score_b)),
                _ => None,
            });
            assert_eq!(
                alice_round, bob_round,
                "both players should observe the same final round result"
            );
            let (_winning_team, score_a, score_b) =
                alice_round.expect("the final round should emit RoundWon");
            assert_eq!(
                u16::from(score_a) + u16::from(score_b),
                5,
                "the final score should account for all five rounds"
            );

            let alice_match_end = alice_match_events.iter().find_map(|event| match event {
                ServerControlEvent::MatchEnded {
                    outcome,
                    score_a,
                    score_b,
                    ..
                } => Some((*outcome, *score_a, *score_b)),
                _ => None,
            });
            let bob_match_end = bob_match_events.iter().find_map(|event| match event {
                ServerControlEvent::MatchEnded {
                    outcome,
                    score_a,
                    score_b,
                    ..
                } => Some((*outcome, *score_a, *score_b)),
                _ => None,
            });
            assert_eq!(
                alice_match_end, bob_match_end,
                "both players should observe the same match outcome"
            );
            let (outcome, score_a, score_b) =
                alice_match_end.expect("the match should emit MatchEnded");
            assert!(matches!(
                outcome,
                MatchOutcome::TeamAWin | MatchOutcome::TeamBWin
            ));
            assert_eq!(
                u16::from(score_a) + u16::from(score_b),
                5,
                "the match score should reflect all five rounds"
            );
            final_outcome = Some(outcome);
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
    let outcome = final_outcome.expect("the match should have completed with an outcome");
    assert!(alice_return_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if match outcome {
                MatchOutcome::TeamAWin =>
                    record.wins == 1 && record.losses == 0 && record.no_contests == 0,
                MatchOutcome::TeamBWin =>
                    record.wins == 0 && record.losses == 1 && record.no_contests == 0,
                MatchOutcome::NoContest => false,
            }
    )));
    assert!(bob_return_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if match outcome {
                MatchOutcome::TeamAWin =>
                    record.wins == 0 && record.losses == 1 && record.no_contests == 0,
                MatchOutcome::TeamBWin =>
                    record.wins == 1 && record.losses == 0 && record.no_contests == 0,
                MatchOutcome::NoContest => false,
            }
    )));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    server.shutdown().await;
}
