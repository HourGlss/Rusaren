use super::*;

#[test]
fn simulation_error_display_covers_all_variants() {
    let cases = [
        (
            SimulationError::DuplicatePlayer(player_id(1)),
            "player 1 appears more than once in the simulation",
        ),
        (
            SimulationError::PlayerMissing(player_id(2)),
            "player 2 is not part of the simulation",
        ),
        (
            SimulationError::PlayerAlreadyDefeated(player_id(3)),
            "player 3 is already defeated",
        ),
        (
            SimulationError::InvalidHitPoints {
                player_id: player_id(4),
                hit_points: 0,
            },
            "player 4 must start with positive hit points, got 0",
        ),
        (
            SimulationError::MovementComponentOutOfRange {
                axis: "x",
                value: 2,
            },
            "movement component x=2 is outside -1..=1",
        ),
        (
            SimulationError::InvalidSkillSlot(6),
            "skill slot 6 is outside the supported range 1..=5",
        ),
        (
            SimulationError::SkillSlotEmpty(3),
            "skill slot 3 is not equipped",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn update_aim_reports_zero_same_changed_missing_and_defeated_cases() {
    let content = content();
    let mut world = world(
        &content,
        vec![seed(
            &content,
            1,
            "Alice",
            TeamSide::TeamA,
            SkillTree::Mage,
            [None, None, None, None, None],
        )],
    );

    assert_eq!(world.update_aim(player_id(1), 0, 0), Ok(false));
    assert_eq!(
        world.update_aim(player_id(1), DEFAULT_AIM_X, DEFAULT_AIM_Y),
        Ok(false)
    );
    assert_eq!(world.update_aim(player_id(1), 0, 120), Ok(true));
    assert_eq!(
        world.player_state(player_id(1)).expect("alice state").aim_y,
        120
    );
    assert_eq!(world.update_aim(player_id(1), 60, 120), Ok(true));
    assert_eq!(
        world.player_state(player_id(1)).expect("alice state").aim_x,
        60
    );
    assert_eq!(world.update_aim(player_id(1), 60, 120), Ok(false));
    assert_eq!(
        world.update_aim(player_id(9), 10, 0),
        Err(SimulationError::PlayerMissing(player_id(9)))
    );

    world.players.get_mut(&player_id(1)).expect("player").alive = false;
    assert_eq!(
        world.update_aim(player_id(1), 60, 0),
        Err(SimulationError::PlayerAlreadyDefeated(player_id(1)))
    );
}

#[test]
fn accessors_and_team_defeat_state_report_runtime_state_exactly() {
    let content = content();
    let mut world = world(
        &content,
        vec![
            seed(
                &content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Mage,
                [Some(choice(SkillTree::Mage, 1)), None, None, None, None],
            ),
            seed(
                &content,
                2,
                "Bob",
                TeamSide::TeamB,
                SkillTree::Warrior,
                [None, None, None, None, None],
            ),
        ],
    );

    assert_eq!(world.players().len(), 2);
    assert_eq!(world.arena_width_units(), content.map().width_units);
    assert_eq!(world.arena_height_units(), content.map().height_units);
    assert!(world.projectiles().is_empty());
    assert!(!world.is_team_defeated(TeamSide::TeamA));
    assert!(!world.is_team_defeated(TeamSide::TeamB));

    world.queue_cast(player_id(1), 1).expect("projectile cast");
    let _ = world.tick(COMBAT_FRAME_MS);
    assert_eq!(world.projectiles().len(), 1);

    world.players.get_mut(&player_id(1)).expect("alice").alive = false;
    assert!(world.is_team_defeated(TeamSide::TeamA));
    assert!(!world.is_team_defeated(TeamSide::TeamB));
}
