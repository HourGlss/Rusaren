use super::*;
use proptest::prelude::*;

fn player_id(raw: u32) -> PlayerId {
    PlayerId::new(raw).expect("valid player id")
}

fn player_name(raw: &str) -> PlayerName {
    PlayerName::new(raw).expect("valid player name")
}

fn lobby() -> Lobby {
    Lobby::new(LobbyId::new(1).expect("valid lobby id"))
}

#[test]
fn add_player_accepts_unique_players_and_rejects_duplicates() {
    let mut lobby = lobby();
    assert_eq!(
        lobby.add_player(player_id(1), player_name("Alice"), PlayerRecord::new()),
        Ok(LobbyEvent::PlayerJoined {
            player_id: player_id(1),
        })
    );

    assert_eq!(
        lobby.add_player(player_id(1), player_name("Alice"), PlayerRecord::new()),
        Err(LobbyError::DuplicatePlayer(player_id(1)))
    );
}

#[test]
fn add_player_starts_in_not_ready_without_a_team() {
    let mut lobby = lobby();
    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");

    let player = lobby.player(player_id(1)).expect("player should exist");
    assert_eq!(player.team, None);
    assert_eq!(player.ready_state, ReadyState::NotReady);
}

#[test]
fn select_team_assigns_team_and_resets_ready_state() {
    let mut lobby = lobby();
    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .select_team(player_id(1), TeamSide::TeamA)
        .expect("team selection should succeed");
    lobby
        .set_ready(player_id(1), ReadyState::Ready)
        .expect("ready should succeed");

    let events = lobby
        .select_team(player_id(1), TeamSide::TeamB)
        .expect("changing teams should succeed");

    assert_eq!(
        events,
        vec![LobbyEvent::TeamSelected {
            player_id: player_id(1),
            team: TeamSide::TeamB,
            ready_reset: true,
        }]
    );
    assert_eq!(
        lobby
            .player(player_id(1))
            .expect("player should exist")
            .ready_state,
        ReadyState::NotReady
    );
}

#[test]
fn select_team_rejects_missing_players_and_locked_rosters() {
    let mut lobby = lobby();
    assert_eq!(
        lobby.select_team(player_id(7), TeamSide::TeamA),
        Err(LobbyError::PlayerMissing(player_id(7)))
    );

    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .select_team(player_id(1), TeamSide::TeamA)
        .expect("team selection should work");
    lobby
        .select_team(player_id(2), TeamSide::TeamB)
        .expect("team selection should work");
    lobby
        .set_ready(player_id(1), ReadyState::Ready)
        .expect("ready should work");
    lobby
        .set_ready(player_id(2), ReadyState::Ready)
        .expect("ready should start countdown");

    assert_eq!(
        lobby.select_team(player_id(1), TeamSide::TeamA),
        Err(LobbyError::LobbyLocked)
    );
}

#[test]
fn set_ready_requires_a_team_and_a_real_player() {
    let mut lobby = lobby();
    assert_eq!(
        lobby.set_ready(player_id(9), ReadyState::Ready),
        Err(LobbyError::PlayerMissing(player_id(9)))
    );

    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");
    assert_eq!(
        lobby.set_ready(player_id(1), ReadyState::Ready),
        Err(LobbyError::TeamRequiredForReady(player_id(1)))
    );
}

#[test]
fn set_ready_starts_the_countdown_once_both_teams_are_ready() {
    let mut lobby = lobby();
    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .select_team(player_id(1), TeamSide::TeamA)
        .expect("team selection should work");
    lobby
        .select_team(player_id(2), TeamSide::TeamB)
        .expect("team selection should work");

    assert_eq!(
        lobby
            .set_ready(player_id(1), ReadyState::Ready)
            .expect("ready should succeed"),
        vec![LobbyEvent::ReadyChanged {
            player_id: player_id(1),
            ready: ReadyState::Ready,
        }]
    );

    let events = lobby
        .set_ready(player_id(2), ReadyState::Ready)
        .expect("second ready should start countdown");

    assert_eq!(
        events,
        vec![
            LobbyEvent::ReadyChanged {
                player_id: player_id(2),
                ready: ReadyState::Ready,
            },
            LobbyEvent::LaunchCountdownStarted {
                seconds_remaining: LAUNCH_COUNTDOWN_SECONDS,
                roster: vec![
                    TeamAssignment {
                        player_id: player_id(1),
                        player_name: player_name("Alice"),
                        record: PlayerRecord::new(),
                        team: TeamSide::TeamA,
                    },
                    TeamAssignment {
                        player_id: player_id(2),
                        player_name: player_name("Bob"),
                        record: PlayerRecord::new(),
                        team: TeamSide::TeamB,
                    },
                ],
            },
        ]
    );
    assert!(matches!(lobby.phase(), LobbyPhase::LaunchCountdown { .. }));
}

#[test]
fn leaving_during_open_removes_the_player() {
    let mut lobby = lobby();
    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");

    assert_eq!(
        lobby.leave_or_disconnect_player(player_id(1)),
        Ok(LobbyEvent::PlayerLeft {
            player_id: player_id(1),
        })
    );
    assert_eq!(lobby.player_count(), 0);
}

#[test]
fn disconnecting_during_countdown_aborts_the_match_and_unlocks_the_lobby() {
    let mut lobby = lobby();
    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .select_team(player_id(1), TeamSide::TeamA)
        .expect("team selection should work");
    lobby
        .select_team(player_id(2), TeamSide::TeamB)
        .expect("team selection should work");
    lobby
        .set_ready(player_id(1), ReadyState::Ready)
        .expect("ready should work");
    lobby
        .set_ready(player_id(2), ReadyState::Ready)
        .expect("ready should start countdown");

    assert_eq!(
        lobby.leave_or_disconnect_player(player_id(2)),
        Ok(LobbyEvent::MatchAborted {
            player_id: player_id(2),
            player_name: player_name("Bob"),
            message: String::from("Bob has disconnected. Game is over."),
        })
    );
    assert!(matches!(lobby.phase(), LobbyPhase::Open));
    assert_eq!(
        lobby
            .player(player_id(1))
            .expect("remaining player should exist")
            .ready_state,
        ReadyState::NotReady
    );
}

#[test]
fn advance_countdown_ticks_and_then_launches_the_locked_roster() {
    let mut lobby = lobby();
    lobby
        .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
        .expect("player should join");
    lobby
        .select_team(player_id(1), TeamSide::TeamA)
        .expect("team selection should work");
    lobby
        .select_team(player_id(2), TeamSide::TeamB)
        .expect("team selection should work");
    lobby
        .set_ready(player_id(1), ReadyState::Ready)
        .expect("ready should work");
    lobby
        .set_ready(player_id(2), ReadyState::Ready)
        .expect("ready should start countdown");

    for remaining in (2..=LAUNCH_COUNTDOWN_SECONDS).rev() {
        assert_eq!(
            lobby.advance_countdown(),
            Ok(LobbyEvent::LaunchCountdownTick {
                seconds_remaining: remaining - 1,
            })
        );
    }

    match lobby.advance_countdown() {
        Ok(LobbyEvent::MatchLaunchReady { roster }) => {
            assert_eq!(roster.len(), 2);
            assert_eq!(lobby.player_count(), 0);
        }
        other => panic!("unexpected launch result: {other:?}"),
    }
}

#[test]
fn advance_countdown_rejects_calls_when_no_countdown_is_running() {
    let mut lobby = lobby();
    assert_eq!(
        lobby.advance_countdown(),
        Err(LobbyError::CountdownNotRunning)
    );
}

#[test]
fn lobby_error_display_covers_all_variants() {
    let cases = [
        (
            LobbyError::DuplicatePlayer(player_id(1)),
            "player 1 is already in the lobby",
        ),
        (
            LobbyError::PlayerMissing(player_id(2)),
            "player 2 is not in the lobby",
        ),
        (
            LobbyError::TeamRequiredForReady(player_id(3)),
            "player 3 must join a team before toggling ready",
        ),
        (LobbyError::LobbyLocked, "lobby roster is locked"),
        (
            LobbyError::CountdownNotRunning,
            "launch countdown is not running",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

fn maybe_team() -> impl Strategy<Value = Option<TeamSide>> {
    prop_oneof![
        Just(None),
        Just(Some(TeamSide::TeamA)),
        Just(Some(TeamSide::TeamB)),
    ]
}

proptest! {
    #[test]
    fn prop_launch_countdown_requires_two_ready_players_on_opposing_teams(
        team_one in maybe_team(),
        team_two in maybe_team(),
        ready_one in any::<bool>(),
        ready_two in any::<bool>(),
    ) {
        let mut lobby = lobby();
        lobby
            .add_player(player_id(1), player_name("Alice"), PlayerRecord::new())
            .expect("player should join");
        lobby
            .add_player(player_id(2), player_name("Bob"), PlayerRecord::new())
            .expect("player should join");

        if let Some(team) = team_one {
            lobby
                .select_team(player_id(1), team)
                .expect("team selection should work");
            if ready_one {
                lobby
                    .set_ready(player_id(1), ReadyState::Ready)
                    .expect("ready should work with a team");
            }
        }

        if let Some(team) = team_two {
            lobby
                .select_team(player_id(2), team)
                .expect("team selection should work");
            if ready_two {
                lobby
                    .set_ready(player_id(2), ReadyState::Ready)
                    .expect("ready should work with a team");
            }
        }

        let should_start = ready_one
            && ready_two
            && matches!(
                (team_one, team_two),
                (Some(TeamSide::TeamA), Some(TeamSide::TeamB))
                    | (Some(TeamSide::TeamB), Some(TeamSide::TeamA))
            );

        prop_assert_eq!(
            matches!(lobby.phase(), LobbyPhase::LaunchCountdown { .. }),
            should_start
        );
    }
}
