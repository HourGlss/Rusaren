#![forbid(unsafe_code)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use game_domain::{PlayerId, PlayerName, TeamSide};
use game_net::{
    ArenaDeltaSnapshot, ArenaEffectKind, ArenaMatchPhase, ArenaObstacleKind, ArenaObstacleSnapshot,
    ArenaPlayerSnapshot, ArenaProjectileSnapshot, ArenaStateSnapshot, ArenaStatusKind,
    ArenaStatusSnapshot, ServerControlEvent,
};

fn player_id(raw: u32) -> PlayerId {
    must(PlayerId::new(raw), "bench player ids should be valid")
}

fn player_name(raw: &str) -> PlayerName {
    must(PlayerName::new(raw), "bench names should be valid")
}

fn sample_players() -> Vec<ArenaPlayerSnapshot> {
    vec![
        ArenaPlayerSnapshot {
            player_id: player_id(1),
            player_name: player_name("Alice"),
            team: TeamSide::TeamA,
            x: -620,
            y: 220,
            aim_x: 90,
            aim_y: -40,
            hit_points: 92,
            max_hit_points: 100,
            mana: 64,
            max_mana: 100,
            alive: true,
            unlocked_skill_slots: 3,
            primary_cooldown_remaining_ms: 180,
            primary_cooldown_total_ms: 650,
            slot_cooldown_remaining_ms: [0, 0, 800, 0, 0],
            slot_cooldown_total_ms: [700, 1700, 2200, 0, 0],
            active_statuses: vec![
                ArenaStatusSnapshot {
                    source: player_id(4),
                    slot: 1,
                    kind: ArenaStatusKind::Poison,
                    stacks: 2,
                    remaining_ms: 1800,
                },
                ArenaStatusSnapshot {
                    source: player_id(3),
                    slot: 1,
                    kind: ArenaStatusKind::Haste,
                    stacks: 1,
                    remaining_ms: 900,
                },
            ],
        },
        ArenaPlayerSnapshot {
            player_id: player_id(2),
            player_name: player_name("Bob"),
            team: TeamSide::TeamB,
            x: 620,
            y: -220,
            aim_x: -90,
            aim_y: 40,
            hit_points: 76,
            max_hit_points: 100,
            mana: 48,
            max_mana: 100,
            alive: true,
            unlocked_skill_slots: 4,
            primary_cooldown_remaining_ms: 0,
            primary_cooldown_total_ms: 550,
            slot_cooldown_remaining_ms: [400, 0, 0, 0, 0],
            slot_cooldown_total_ms: [900, 1700, 2200, 1800, 3200],
            active_statuses: vec![
                ArenaStatusSnapshot {
                    source: player_id(1),
                    slot: 1,
                    kind: ArenaStatusKind::Silence,
                    stacks: 1,
                    remaining_ms: 600,
                },
                ArenaStatusSnapshot {
                    source: player_id(5),
                    slot: 1,
                    kind: ArenaStatusKind::Stun,
                    stacks: 1,
                    remaining_ms: 300,
                },
            ],
        },
    ]
}

fn sample_obstacles() -> Vec<ArenaObstacleSnapshot> {
    vec![
        ArenaObstacleSnapshot {
            kind: ArenaObstacleKind::Shrub,
            center_x: -220,
            center_y: -150,
            half_width: 92,
            half_height: 92,
        },
        ArenaObstacleSnapshot {
            kind: ArenaObstacleKind::Pillar,
            center_x: -220,
            center_y: -150,
            half_width: 70,
            half_height: 70,
        },
        ArenaObstacleSnapshot {
            kind: ArenaObstacleKind::Shrub,
            center_x: 220,
            center_y: 150,
            half_width: 92,
            half_height: 92,
        },
        ArenaObstacleSnapshot {
            kind: ArenaObstacleKind::Pillar,
            center_x: 220,
            center_y: 150,
            half_width: 70,
            half_height: 70,
        },
    ]
}

fn sample_projectiles() -> Vec<ArenaProjectileSnapshot> {
    vec![
        ArenaProjectileSnapshot {
            owner: player_id(1),
            slot: 1,
            kind: ArenaEffectKind::SkillShot,
            x: -300,
            y: 100,
            radius: 18,
        },
        ArenaProjectileSnapshot {
            owner: player_id(6),
            slot: 1,
            kind: ArenaEffectKind::SkillShot,
            x: 280,
            y: -80,
            radius: 16,
        },
    ]
}

fn sample_full_snapshot() -> ServerControlEvent {
    ServerControlEvent::ArenaStateSnapshot {
        snapshot: ArenaStateSnapshot {
            phase: ArenaMatchPhase::Combat,
            phase_seconds_remaining: None,
            width: 1800,
            height: 1200,
            tile_units: 50,
            visible_tiles: vec![0b0011_1111, 0b0000_0011],
            explored_tiles: vec![0b1111_1111, 0b0000_1111],
            obstacles: sample_obstacles(),
            players: sample_players(),
            projectiles: sample_projectiles(),
        },
    }
}

fn sample_delta_snapshot() -> ServerControlEvent {
    ServerControlEvent::ArenaDeltaSnapshot {
        snapshot: ArenaDeltaSnapshot {
            phase: ArenaMatchPhase::Combat,
            phase_seconds_remaining: None,
            tile_units: 50,
            visible_tiles: vec![0b0011_1111, 0b0000_0011],
            explored_tiles: vec![0b1111_1111, 0b0000_1111],
            players: sample_players(),
            projectiles: sample_projectiles(),
        },
    }
}

fn bench_snapshot_codec(c: &mut Criterion) {
    let full_snapshot = sample_full_snapshot();
    let delta_snapshot = sample_delta_snapshot();

    c.bench_function("encode_full_snapshot_packet", |b| {
        b.iter(|| {
            black_box(
                full_snapshot
                    .clone()
                    .encode_packet(1, 42)
                    .unwrap_or_else(|error| panic!("full snapshot should encode: {error}")),
            );
        });
    });

    let full_packet = full_snapshot
        .clone()
        .encode_packet(1, 42)
        .unwrap_or_else(|error| panic!("full snapshot should encode: {error}"));
    c.bench_function("decode_full_snapshot_packet", |b| {
        b.iter(|| {
            black_box(
                ServerControlEvent::decode_packet(&full_packet)
                    .unwrap_or_else(|error| panic!("full snapshot should decode: {error}")),
            );
        });
    });

    c.bench_function("encode_delta_snapshot_packet", |b| {
        b.iter(|| {
            black_box(
                delta_snapshot
                    .clone()
                    .encode_packet(2, 42)
                    .unwrap_or_else(|error| panic!("delta snapshot should encode: {error}")),
            );
        });
    });

    let delta_packet = delta_snapshot
        .clone()
        .encode_packet(2, 42)
        .unwrap_or_else(|error| panic!("delta snapshot should encode: {error}"));
    c.bench_function("decode_delta_snapshot_packet", |b| {
        b.iter(|| {
            black_box(
                ServerControlEvent::decode_packet(&delta_packet)
                    .unwrap_or_else(|error| panic!("delta snapshot should decode: {error}")),
            );
        });
    });
}

fn must<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

criterion_group!(benches, bench_snapshot_codec);
criterion_main!(benches);
