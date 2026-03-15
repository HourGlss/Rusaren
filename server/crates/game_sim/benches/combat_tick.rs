#![forbid(unsafe_code)]

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use game_content::GameContent;
use game_domain::{
    PlayerId, PlayerName, PlayerRecord, SkillChoice, SkillTree, TeamAssignment, TeamSide,
};
use game_sim::{MovementIntent, SimPlayerSeed, SimulationWorld, COMBAT_FRAME_MS};

fn player_id(raw: u32) -> PlayerId {
    must(PlayerId::new(raw), "bench player ids should be valid")
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    must(
        SkillChoice::new(tree, tier),
        "bench skill choices should be valid",
    )
}

fn assignment(raw_id: u32, raw_name: &str, team: TeamSide) -> TeamAssignment {
    TeamAssignment {
        player_id: player_id(raw_id),
        player_name: must(PlayerName::new(raw_name), "bench names should be valid"),
        record: PlayerRecord::new(),
        team,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn seed(
    content: &GameContent,
    raw_id: u32,
    raw_name: &str,
    team: TeamSide,
    primary_tree: SkillTree,
    slot_one_choice: SkillChoice,
) -> SimPlayerSeed {
    SimPlayerSeed {
        assignment: assignment(raw_id, raw_name, team),
        hit_points: 100,
        melee: content
            .skills()
            .melee_for(&primary_tree)
            .unwrap_or_else(|| panic!("bench melee should exist for {primary_tree}"))
            .clone(),
        skills: [
            Some(
                content
                    .skills()
                    .resolve(&slot_one_choice)
                    .unwrap_or_else(|| panic!("bench slot one skill should exist"))
                    .clone(),
            ),
            None,
            None,
            None,
            None,
        ],
    }
}

fn build_world(content: &GameContent) -> SimulationWorld {
    SimulationWorld::new(
        vec![
            seed(
                content,
                1,
                "Alice",
                TeamSide::TeamA,
                SkillTree::Warrior,
                skill(SkillTree::Warrior, 1),
            ),
            seed(
                content,
                2,
                "Bea",
                TeamSide::TeamA,
                SkillTree::Mage,
                skill(SkillTree::Mage, 1),
            ),
            seed(
                content,
                3,
                "ClericA",
                TeamSide::TeamA,
                SkillTree::Cleric,
                skill(SkillTree::Cleric, 2),
            ),
            seed(
                content,
                4,
                "Drake",
                TeamSide::TeamB,
                SkillTree::Rogue,
                skill(SkillTree::Rogue, 1),
            ),
            seed(
                content,
                5,
                "Eve",
                TeamSide::TeamB,
                SkillTree::Warrior,
                skill(SkillTree::Warrior, 2),
            ),
            seed(
                content,
                6,
                "Faith",
                TeamSide::TeamB,
                SkillTree::Cleric,
                skill(SkillTree::Cleric, 4),
            ),
        ],
        content.map(),
    )
    .unwrap_or_else(|error| panic!("bench world should build: {error}"))
}

fn must<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

fn bench_simulation_tick(c: &mut Criterion) {
    let content = must(GameContent::bundled(), "bundled content should load");
    let base_world = build_world(&content);
    let move_right = must(MovementIntent::new(1, 0), "movement should be valid");
    let move_left = must(MovementIntent::new(-1, 0), "movement should be valid");

    c.bench_function("simulation_tick_six_players", |b| {
        b.iter_batched(
            || base_world.clone(),
            |mut world| {
                for player in [player_id(1), player_id(2), player_id(3)] {
                    must(world.submit_input(player, move_right), "movement input");
                    must(world.update_aim(player, 120, 0), "aim update");
                    must(world.queue_cast(player, 1), "cast queue");
                }
                for player in [player_id(4), player_id(5), player_id(6)] {
                    must(world.submit_input(player, move_left), "movement input");
                    must(world.update_aim(player, -120, 0), "aim update");
                    must(world.queue_cast(player, 1), "cast queue");
                }
                let events = world.tick(COMBAT_FRAME_MS);
                black_box(events);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_simulation_tick);
criterion_main!(benches);
