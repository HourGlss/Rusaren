use super::*;
use crate::combat_log::{CombatLogEvent, CombatLogOutcome, CombatLogPhase, CombatLogTeam};

#[test]
fn end_to_end_game_lobby_countdown_and_match_start_work_via_fake_clients() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);
    assert_eq!(match_id.get(), 1);
}

#[test]
fn fake_clients_receive_delta_snapshots_with_runtime_status_and_mana_state() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("alice skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Warrior, 1))
        .expect("bob skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice skill events");
    let _ = bob.drain_events(&mut transport).expect("bob skill events");

    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob pre-combat events");

    let (alice_events, bob_events) =
        cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, 1);
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaDeltaSnapshot { snapshot }
            if snapshot.players.iter().any(|player| player.mana < player.max_mana)
    )));
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ArenaDeltaSnapshot { snapshot }
            if snapshot.players.iter().any(|player| {
                player.player_id == bob.player_id().expect("bob id")
                    && player.active_statuses.iter().any(|status| status.kind == ArenaStatusKind::Poison)
            })
    )));
}

#[test]
fn round_transition_rebuilds_a_clean_world_for_the_next_combat_phase() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("alice round-one skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Warrior, 1))
        .expect("bob round-one skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice round-one skill events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob round-one skill events");

    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice round-one pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob round-one pre-combat events");

    let _ = cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, 1);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Rogue, 2))
        .expect("alice round-two skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Warrior, 2))
        .expect("bob round-two skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice round-two skill events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob round-two skill events");

    server.advance_seconds(&mut transport, 5);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice round-two combat events");
    let bob_events = bob
        .drain_events(&mut transport)
        .expect("bob round-two combat events");

    for snapshot in [
        arena_state_snapshot(&alice_events).expect("alice should receive a fresh arena snapshot"),
        arena_state_snapshot(&bob_events).expect("bob should receive a fresh arena snapshot"),
    ] {
        assert!(snapshot.projectiles.is_empty());
        assert!(snapshot.players.iter().all(|player| {
            player.hit_points == player.max_hit_points
                && player.mana == player.max_mana
                && player.primary_cooldown_remaining_ms == 0
                && player
                    .slot_cooldown_remaining_ms
                    .iter()
                    .all(|remaining| *remaining == 0)
                && player.active_statuses.is_empty()
        }));
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn end_to_end_skill_pick_round_flow_match_end_and_quit_work_via_fake_clients() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    for tier in 1..=5 {
        alice
            .choose_skill(&mut transport, skill(SkillTree::Mage, tier))
            .expect("alice skill");
        bob.choose_skill(&mut transport, skill(SkillTree::Rogue, tier))
            .expect("bob skill");
        server.pump_transport(&mut transport);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice skill events");
        let bob_events = bob.drain_events(&mut transport).expect("bob skill events");
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::PreCombatStarted {
                seconds_remaining: 5
            }
        )));
        assert!(bob_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::PreCombatStarted {
                seconds_remaining: 5
            }
        )));

        server.advance_seconds(&mut transport, 5);
        let alice_events = alice
            .drain_events(&mut transport)
            .expect("alice pre-combat events");
        assert!(alice_events
            .iter()
            .any(|event| matches!(event, ServerControlEvent::CombatStarted)));
        let _ = bob
            .drain_events(&mut transport)
            .expect("bob pre-combat events");

        let (alice_events, bob_events) =
            cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, tier);
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::ArenaEffectBatch { effects }
                if effects.iter().any(|effect| effect.slot == 1)
        )));
        assert!(alice_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::RoundWon {
                round,
                winning_team: TeamSide::TeamA,
                ..
            } if round.get() == tier
        )));
        assert!(bob_events.iter().any(|event| matches!(
            event,
            ServerControlEvent::RoundWon { round, .. } if round.get() == tier
        )));

        if tier == 5 {
            assert!(alice_events.iter().any(|event| matches!(
                event,
                ServerControlEvent::MatchEnded {
                    outcome: MatchOutcome::TeamAWin,
                    score_a: 5,
                    score_b: 0,
                    ..
                }
            )));
            assert!(bob_events.iter().any(|event| matches!(
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

    alice
        .quit_to_central_lobby(&mut transport)
        .expect("alice quit");
    bob.quit_to_central_lobby(&mut transport).expect("bob quit");
    server.pump_transport(&mut transport);

    let alice_events = alice.drain_events(&mut transport).expect("alice return");
    let bob_events = bob.drain_events(&mut transport).expect("bob return");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 1 && record.losses == 0 && record.no_contests == 0
    )));
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 0 && record.losses == 1 && record.no_contests == 0
    )));
}

#[test]
fn end_to_end_skill_pick_rejects_tier_skips_but_accepts_the_next_valid_tier() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 5))
        .expect("invalid skill packet should still encode");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice invalid skill response");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "skill progression for Mage expected tier 1 but received tier 5"
    )));

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
        .expect("alice valid tier one");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("bob valid tier one");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice first-round skill events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob first-round skill events");
    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice first-round pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob first-round pre-combat events");
    let _ = cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, 1);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 3))
        .expect("invalid second-round skill packet should still encode");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice second invalid skill response");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "skill progression for Mage expected tier 2 but received tier 3"
    )));

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 2))
        .expect("alice valid tier two");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 2))
        .expect("bob valid tier two");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice second-round skill events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::PreCombatStarted {
            seconds_remaining: 5
        }
    )));
}

#[test]
fn end_to_end_disconnect_ends_the_match_as_no_contest() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    server
        .disconnect_player(
            &mut transport,
            bob.player_id()
                .expect("bob should be connected before disconnect"),
        )
        .expect("disconnect should work");
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice disconnect events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::MatchEnded {
            outcome: MatchOutcome::NoContest,
            message,
            ..
        } if message == "Bob has disconnected. Game is over."
    )));

    alice
        .quit_to_central_lobby(&mut transport)
        .expect("alice quit");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice return events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::ReturnedToCentralLobby { record }
            if record.wins == 0 && record.losses == 0 && record.no_contests == 1
    )));
    assert!(!server.matches.contains_key(&match_id));
}

#[test]
fn end_to_end_rejects_invalid_sequences_and_wrong_state_commands() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    bob.leave_game_lobby(&mut transport).expect("leave packet");
    server.pump_transport(&mut transport);
    let bob_events = bob.drain_events(&mut transport).expect("bob error");
    assert!(bob_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "player is not inside a game lobby"
    )));

    let stale = ClientControlCommand::CreateGameLobby
        .encode_packet(1, 0)
        .expect("stale packet");
    transport.send_from_client(alice.connection_id(), stale);
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice stale error");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message } if message.contains("incoming sequence")
    )));
}

#[test]
fn end_to_end_rejects_stale_client_input_ticks_even_with_new_packet_sequences() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
        .expect("alice skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("bob skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice skill events");
    let _ = bob.drain_events(&mut transport).expect("bob skill events");

    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob pre-combat events");

    alice
        .send_input(&mut transport, movement_input_frame(7, 1, 0), 1)
        .expect("first movement input");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice first input events");

    alice
        .send_input(&mut transport, movement_input_frame(7, 1, 0), 2)
        .expect("duplicate tick input");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice duplicate tick events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "client_input_tick 7 is not newer than 7"
    )));
}

#[test]
fn end_to_end_rejects_locked_skill_slot_cast_inputs_even_when_the_packet_is_valid() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
        .expect("alice skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("bob skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice skill events");
    let _ = bob.drain_events(&mut transport).expect("bob skill events");

    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob pre-combat events");

    alice
        .send_input(&mut transport, slot_cast_input(1, 5), 1)
        .expect("locked-slot cast packet");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice locked-slot events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "skill slot 5 is not unlocked for round 1"
    )));
}

#[test]
fn movement_spam_cannot_move_farther_than_one_simulated_frame() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
        .expect("alice skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("bob skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice skill events");
    let _ = bob.drain_events(&mut transport).expect("bob skill events");

    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob pre-combat events");

    let alice_id = alice.player_id().expect("alice id");
    let starting_x = player_x(&server, match_id, alice_id);
    for tick in 1..=10 {
        alice
            .send_input(&mut transport, movement_input_frame(tick, 1, 0), tick)
            .expect("movement spam packet");
    }
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice input events");

    server.advance_millis(&mut transport, COMBAT_FRAME_MS);
    let ending_x = player_x(&server, match_id, alice_id);
    let actual_distance = ending_x - starting_x;
    let expected_distance = i16::try_from(
        u32::from(PLAYER_MOVE_SPEED_UNITS_PER_SECOND) * u32::from(COMBAT_FRAME_MS) / 1000,
    )
    .unwrap_or(i16::MAX);

    assert_eq!(
        actual_distance, expected_distance,
        "one combat frame of movement input should only move one frame's worth of distance"
    );
}

#[test]
fn end_to_end_rejects_out_of_range_movement_components() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);

    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

    alice
        .choose_skill(&mut transport, skill(SkillTree::Mage, 1))
        .expect("alice skill");
    bob.choose_skill(&mut transport, skill(SkillTree::Rogue, 1))
        .expect("bob skill");
    server.pump_transport(&mut transport);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice skill events");
    let _ = bob.drain_events(&mut transport).expect("bob skill events");

    server.advance_seconds(&mut transport, 5);
    let _ = alice
        .drain_events(&mut transport)
        .expect("alice pre-combat events");
    let _ = bob
        .drain_events(&mut transport)
        .expect("bob pre-combat events");

    alice
        .send_input(&mut transport, movement_input_frame(1, 2, 0), 1)
        .expect("invalid movement packet");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice invalid movement events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "move_horizontal_q=2 is outside the allowed range -1..=1"
    )));
}

#[test]
fn end_to_end_rejects_input_frames_outside_combat() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let _ = launch_match(&mut server, &mut transport, &mut alice, &mut bob);
    let alice_id = alice.player_id().expect("alice id");

    server.handle_input_frame(&mut transport, alice_id, movement_input_frame(1, 1, 0));
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice non-combat input events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "input frames are only accepted during combat"
    )));
}

#[test]
fn end_to_end_rejects_quit_button_frames_during_combat() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let _ = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Mage, 1),
        skill(SkillTree::Rogue, 1),
    );

    alice
        .send_input(&mut transport, quit_input_frame(1), 1)
        .expect("quit input packet");
    server.pump_transport(&mut transport);
    let alice_events = alice
        .drain_events(&mut transport)
        .expect("alice quit-button events");
    assert!(alice_events.iter().any(|event| matches!(
        event,
        ServerControlEvent::Error { message }
            if message == "quit-to-lobby input is not valid during combat"
    )));
}

#[test]
fn primary_and_cast_button_paths_are_independent() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Mage, 1),
        skill(SkillTree::Rogue, 1),
    );
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");

    server.handle_input_frame(
        &mut transport,
        alice_id,
        ValidatedInputFrame::new(1, 0, 0, 120, 0, BUTTON_PRIMARY, 0).expect("primary frame"),
    );
    server.advance_millis(&mut transport, COMBAT_FRAME_MS);
    let alice_events = alice.drain_events(&mut transport).expect("alice events");
    let alice_state = server.matches[&match_id]
        .world
        .player_state(alice_id)
        .expect("alice state");
    assert!(
        alice_events
            .iter()
            .all(|event| !matches!(event, ServerControlEvent::Error { .. })),
        "primary-only inputs should not trigger cast validation errors"
    );
    assert!(alice_state.primary_cooldown_remaining_ms > 0);
    assert!(alice_state
        .slot_cooldown_remaining_ms
        .iter()
        .all(|remaining| *remaining == 0));

    server.handle_input_frame(
        &mut transport,
        bob_id,
        ValidatedInputFrame::new(1, 0, 0, -120, 0, BUTTON_CAST, 1).expect("cast frame"),
    );
    server.advance_millis(&mut transport, COMBAT_FRAME_MS);
    let bob_events = bob.drain_events(&mut transport).expect("bob events");
    let bob_state = server.matches[&match_id]
        .world
        .player_state(bob_id)
        .expect("bob state");
    assert!(
        bob_events
            .iter()
            .all(|event| !matches!(event, ServerControlEvent::Error { .. })),
        "cast-only inputs should not trigger primary attacks"
    );
    assert_eq!(bob_state.primary_cooldown_remaining_ms, 0);
    assert!(bob_state.slot_cooldown_remaining_ms[0] > 0);
}

#[test]
#[allow(clippy::too_many_lines)]
fn persistent_combat_logs_replay_a_complete_match_flow() {
    fn assert_full_match_replay(entries: &[crate::combat_log::CombatLogEntry], match_id: MatchId) {
        assert!(
            entries.iter().all(|entry| entry.match_id == match_id.get()),
            "every persisted entry should belong to the requested match"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry.event, CombatLogEvent::MatchStarted { .. }))
                .count(),
            1,
            "exactly one match-start row should be recorded"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry.event, CombatLogEvent::SkillPicked { .. }))
                .count(),
            10,
            "five rounds with two players should emit ten skill-pick rows"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry.event, CombatLogEvent::PreCombatStarted { .. }))
                .count(),
            5,
            "each round should emit one pre-combat countdown row"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry.event, CombatLogEvent::CombatStarted))
                .count(),
            5,
            "each round should emit one combat-start row"
        );

        let round_wins = entries
            .iter()
            .filter_map(|entry| match &entry.event {
                CombatLogEvent::RoundWon {
                    round,
                    winning_team,
                    score_a,
                    score_b,
                } => Some((*round, *winning_team, *score_a, *score_b)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            round_wins,
            vec![
                (1, CombatLogTeam::TeamA, 1, 0),
                (2, CombatLogTeam::TeamA, 2, 0),
                (3, CombatLogTeam::TeamA, 3, 0),
                (4, CombatLogTeam::TeamA, 4, 0),
                (5, CombatLogTeam::TeamA, 5, 0),
            ],
            "the persisted round winners should reconstruct the full five-round scoreline"
        );

        let match_ended = entries
            .iter()
            .find_map(|entry| match &entry.event {
                CombatLogEvent::MatchEnded {
                    outcome,
                    score_a,
                    score_b,
                    message,
                } => Some((*outcome, *score_a, *score_b, message.clone())),
                _ => None,
            })
            .expect("the durable log should contain a match-ended row");
        assert_eq!(
            match_ended,
            (
                CombatLogOutcome::TeamAWin,
                5,
                0,
                String::from("Team A wins 5-0 after round 5."),
            ),
            "the persisted match-ended row should reconstruct the final result without consulting runtime state"
        );

        assert!(
            entries.iter().any(|entry| matches!(
                entry.event,
                CombatLogEvent::CastStarted { .. }
                    | CombatLogEvent::CastCompleted { .. }
                    | CombatLogEvent::ImpactHit { .. }
                    | CombatLogEvent::DamageApplied { .. }
            ) && entry.phase == CombatLogPhase::Combat
                && entry.frame_index > 0),
            "combat logs should include real combat-timeline events with nonzero frame indices"
        );
    }

    let path = temp_path("server-app-combat-log-replay");
    let combat_log_path = companion_combat_log_path(&path);
    remove_if_exists(&path);
    remove_if_exists(&combat_log_path);

    let (match_id, before_reload_entries) = {
        let mut server = ServerApp::new_persistent(&path).expect("persistent server should start");
        let mut transport = InMemoryTransport::new();
        let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
        let match_id = launch_match(&mut server, &mut transport, &mut alice, &mut bob);

        for tier in 1..=5 {
            alice
                .choose_skill(&mut transport, skill(SkillTree::Mage, tier))
                .expect("alice skill");
            bob.choose_skill(&mut transport, skill(SkillTree::Rogue, tier))
                .expect("bob skill");
            server.pump_transport(&mut transport);
            let _ = alice
                .drain_events(&mut transport)
                .expect("alice skill events");
            let _ = bob.drain_events(&mut transport).expect("bob skill events");

            server.advance_seconds(&mut transport, 5);
            let _ = alice
                .drain_events(&mut transport)
                .expect("alice pre-combat events");
            let _ = bob
                .drain_events(&mut transport)
                .expect("bob pre-combat events");
            let _ = cast_until_round_won(&mut server, &mut transport, &mut alice, &mut bob, tier);
        }

        let entries = server
            .combat_log_entries(match_id)
            .expect("combat logs should be queryable");
        assert_full_match_replay(&entries, match_id);
        (match_id, entries)
    };

    let reloaded = ServerApp::new_persistent(&path).expect("persistent server should reload");
    let after_reload_entries = reloaded
        .combat_log_entries(match_id)
        .expect("reloaded combat logs should be queryable");
    assert_eq!(
        after_reload_entries, before_reload_entries,
        "reloading the persistent app should preserve the exact durable combat log rows"
    );
    assert_full_match_replay(&after_reload_entries, match_id);

    drop(reloaded);
    remove_if_exists(&path);
    remove_if_exists(&combat_log_path);
}
