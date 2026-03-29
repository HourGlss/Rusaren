use std::collections::VecDeque;
use std::time::Duration;

use game_net::{ArenaDeltaSnapshot, ArenaStateSnapshot, ServerControlEvent};
use serde::Serialize;

const MAX_RECENT_TIMING_SAMPLES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutboundPacketKind {
    ControlEvent,
    FullSnapshot,
    DeltaSnapshot,
    EffectBatch,
    CombatTextBatch,
}

impl OutboundPacketKind {
    pub(crate) fn from_event(event: &ServerControlEvent) -> Self {
        match event {
            ServerControlEvent::ArenaStateSnapshot { .. } => Self::FullSnapshot,
            ServerControlEvent::ArenaDeltaSnapshot { .. } => Self::DeltaSnapshot,
            ServerControlEvent::ArenaEffectBatch { .. } => Self::EffectBatch,
            ServerControlEvent::ArenaCombatTextBatch { .. } => Self::CombatTextBatch,
            _ => Self::ControlEvent,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct TimingStatsSnapshot {
    pub sample_count: u64,
    pub last_ms: f64,
    pub avg_ms: f64,
    pub max_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Default)]
pub(crate) struct RollingTimingStats {
    sample_count: u64,
    total_us: u128,
    last_us: u64,
    max_us: u64,
    recent_us: VecDeque<u64>,
}

impl RollingTimingStats {
    pub(crate) fn record_duration(&mut self, duration: Duration) {
        let capped = duration.as_micros().min(u128::from(u64::MAX));
        let micros = u64::try_from(capped).unwrap_or(u64::MAX);
        self.record_micros(micros);
    }

    pub(crate) fn record_micros(&mut self, micros: u64) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.total_us = self.total_us.saturating_add(u128::from(micros));
        self.last_us = micros;
        self.max_us = self.max_us.max(micros);
        self.recent_us.push_back(micros);
        while self.recent_us.len() > MAX_RECENT_TIMING_SAMPLES {
            let _ = self.recent_us.pop_front();
        }
    }

    pub(crate) fn snapshot(&self) -> TimingStatsSnapshot {
        let mut recent = self.recent_us.iter().copied().collect::<Vec<_>>();
        recent.sort_unstable();
        TimingStatsSnapshot {
            sample_count: self.sample_count,
            last_ms: micros_to_ms(self.last_us),
            avg_ms: if self.sample_count == 0 {
                0.0
            } else {
                micros_to_ms_u128(self.total_us / u128::from(self.sample_count))
            },
            max_ms: micros_to_ms(self.max_us),
            p50_ms: percentile_ms(&recent, 0.50),
            p95_ms: percentile_ms(&recent, 0.95),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct PacketStatsSnapshot {
    pub sent_packets: u64,
    pub sent_bytes: u64,
    pub last_packet_bytes: u32,
    pub max_packet_bytes: u32,
}

#[derive(Debug, Default)]
pub(crate) struct PacketStats {
    sent_packets: u64,
    sent_bytes: u64,
    last_packet_bytes: u32,
    max_packet_bytes: u32,
}

impl PacketStats {
    pub(crate) fn record(&mut self, bytes: usize) {
        let capped = bytes.min(usize::try_from(u32::MAX).unwrap_or(usize::MAX));
        let packet_bytes = u32::try_from(capped).unwrap_or(u32::MAX);
        self.sent_packets = self.sent_packets.saturating_add(1);
        self.sent_bytes = self
            .sent_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        self.last_packet_bytes = packet_bytes;
        self.max_packet_bytes = self.max_packet_bytes.max(packet_bytes);
    }

    pub(crate) fn snapshot(&self) -> PacketStatsSnapshot {
        PacketStatsSnapshot {
            sent_packets: self.sent_packets,
            sent_bytes: self.sent_bytes,
            last_packet_bytes: self.last_packet_bytes,
            max_packet_bytes: self.max_packet_bytes,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct SnapshotShapeSnapshot {
    pub width: u16,
    pub height: u16,
    pub footprint_tile_count: u32,
    pub visible_tile_count: u32,
    pub explored_tile_count: u32,
    pub obstacle_count: u16,
    pub deployable_count: u16,
    pub player_count: u16,
    pub projectile_count: u16,
}

impl SnapshotShapeSnapshot {
    pub(crate) fn from_state_snapshot(snapshot: &ArenaStateSnapshot) -> Self {
        Self {
            width: snapshot.width,
            height: snapshot.height,
            footprint_tile_count: count_set_bits(&snapshot.footprint_tiles),
            visible_tile_count: count_set_bits(&snapshot.visible_tiles),
            explored_tile_count: count_set_bits(&snapshot.explored_tiles),
            obstacle_count: u16::try_from(snapshot.obstacles.len()).unwrap_or(u16::MAX),
            deployable_count: u16::try_from(snapshot.deployables.len()).unwrap_or(u16::MAX),
            player_count: u16::try_from(snapshot.players.len()).unwrap_or(u16::MAX),
            projectile_count: u16::try_from(snapshot.projectiles.len()).unwrap_or(u16::MAX),
        }
    }

    pub(crate) fn from_delta_snapshot(snapshot: &ArenaDeltaSnapshot) -> Self {
        Self {
            width: 0,
            height: 0,
            footprint_tile_count: count_set_bits(&snapshot.footprint_tiles),
            visible_tile_count: count_set_bits(&snapshot.visible_tiles),
            explored_tile_count: count_set_bits(&snapshot.explored_tiles),
            obstacle_count: u16::try_from(snapshot.obstacles.len()).unwrap_or(u16::MAX),
            deployable_count: u16::try_from(snapshot.deployables.len()).unwrap_or(u16::MAX),
            player_count: u16::try_from(snapshot.players.len()).unwrap_or(u16::MAX),
            projectile_count: u16::try_from(snapshot.projectiles.len()).unwrap_or(u16::MAX),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct AppDiagnosticsSnapshot {
    pub control_events: PacketStatsSnapshot,
    pub full_snapshots: PacketStatsSnapshot,
    pub delta_snapshots: PacketStatsSnapshot,
    pub effect_batches: PacketStatsSnapshot,
    pub combat_text_batches: PacketStatsSnapshot,
    pub match_full_snapshot_build: TimingStatsSnapshot,
    pub match_delta_snapshot_build: TimingStatsSnapshot,
    pub training_full_snapshot_build: TimingStatsSnapshot,
    pub training_delta_snapshot_build: TimingStatsSnapshot,
    pub last_match_full_snapshot: Option<SnapshotShapeSnapshot>,
    pub last_match_delta_snapshot: Option<SnapshotShapeSnapshot>,
    pub last_training_full_snapshot: Option<SnapshotShapeSnapshot>,
    pub last_training_delta_snapshot: Option<SnapshotShapeSnapshot>,
    pub peak_player_count: u16,
    pub peak_projectile_count: u16,
    pub peak_deployable_count: u16,
    pub peak_visible_tile_count: u32,
}

#[derive(Debug, Default)]
pub(crate) struct AppDiagnostics {
    control_events: PacketStats,
    full_snapshots: PacketStats,
    delta_snapshots: PacketStats,
    effect_batches: PacketStats,
    combat_text_batches: PacketStats,
    match_full_snapshot_build: RollingTimingStats,
    match_delta_snapshot_build: RollingTimingStats,
    training_full_snapshot_build: RollingTimingStats,
    training_delta_snapshot_build: RollingTimingStats,
    last_match_full_snapshot: Option<SnapshotShapeSnapshot>,
    last_match_delta_snapshot: Option<SnapshotShapeSnapshot>,
    last_training_full_snapshot: Option<SnapshotShapeSnapshot>,
    last_training_delta_snapshot: Option<SnapshotShapeSnapshot>,
    peak_player_count: u16,
    peak_projectile_count: u16,
    peak_deployable_count: u16,
    peak_visible_tile_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotBuildKind {
    MatchFull,
    MatchDelta,
    TrainingFull,
    TrainingDelta,
}

impl AppDiagnostics {
    pub(crate) fn record_outbound_packet(&mut self, kind: OutboundPacketKind, bytes: usize) {
        match kind {
            OutboundPacketKind::ControlEvent => self.control_events.record(bytes),
            OutboundPacketKind::FullSnapshot => self.full_snapshots.record(bytes),
            OutboundPacketKind::DeltaSnapshot => self.delta_snapshots.record(bytes),
            OutboundPacketKind::EffectBatch => self.effect_batches.record(bytes),
            OutboundPacketKind::CombatTextBatch => self.combat_text_batches.record(bytes),
        }
    }

    pub(crate) fn record_snapshot_build(
        &mut self,
        kind: SnapshotBuildKind,
        duration: Duration,
        shape: SnapshotShapeSnapshot,
    ) {
        self.peak_player_count = self.peak_player_count.max(shape.player_count);
        self.peak_projectile_count = self.peak_projectile_count.max(shape.projectile_count);
        self.peak_deployable_count = self.peak_deployable_count.max(shape.deployable_count);
        self.peak_visible_tile_count = self.peak_visible_tile_count.max(shape.visible_tile_count);

        match kind {
            SnapshotBuildKind::MatchFull => {
                self.match_full_snapshot_build.record_duration(duration);
                self.last_match_full_snapshot = Some(shape);
            }
            SnapshotBuildKind::MatchDelta => {
                self.match_delta_snapshot_build.record_duration(duration);
                self.last_match_delta_snapshot = Some(shape);
            }
            SnapshotBuildKind::TrainingFull => {
                self.training_full_snapshot_build.record_duration(duration);
                self.last_training_full_snapshot = Some(shape);
            }
            SnapshotBuildKind::TrainingDelta => {
                self.training_delta_snapshot_build.record_duration(duration);
                self.last_training_delta_snapshot = Some(shape);
            }
        }
    }

    pub(crate) fn snapshot(&self) -> AppDiagnosticsSnapshot {
        AppDiagnosticsSnapshot {
            control_events: self.control_events.snapshot(),
            full_snapshots: self.full_snapshots.snapshot(),
            delta_snapshots: self.delta_snapshots.snapshot(),
            effect_batches: self.effect_batches.snapshot(),
            combat_text_batches: self.combat_text_batches.snapshot(),
            match_full_snapshot_build: self.match_full_snapshot_build.snapshot(),
            match_delta_snapshot_build: self.match_delta_snapshot_build.snapshot(),
            training_full_snapshot_build: self.training_full_snapshot_build.snapshot(),
            training_delta_snapshot_build: self.training_delta_snapshot_build.snapshot(),
            last_match_full_snapshot: self.last_match_full_snapshot.clone(),
            last_match_delta_snapshot: self.last_match_delta_snapshot.clone(),
            last_training_full_snapshot: self.last_training_full_snapshot.clone(),
            last_training_delta_snapshot: self.last_training_delta_snapshot.clone(),
            peak_player_count: self.peak_player_count,
            peak_projectile_count: self.peak_projectile_count,
            peak_deployable_count: self.peak_deployable_count,
            peak_visible_tile_count: self.peak_visible_tile_count,
        }
    }
}

fn count_set_bits(mask: &[u8]) -> u32 {
    mask.iter().map(|byte| byte.count_ones()).sum()
}

fn micros_to_ms(micros: u64) -> f64 {
    micros_to_ms_u128(u128::from(micros))
}

fn micros_to_ms_u128(micros: u128) -> f64 {
    micros as f64 / 1000.0
}

fn percentile_ms(sorted_micros: &[u64], quantile: f64) -> f64 {
    if sorted_micros.is_empty() {
        return 0.0;
    }
    let last_index = sorted_micros.len().saturating_sub(1);
    let scaled = (last_index as f64 * quantile).round();
    let index = usize::try_from(scaled as u64)
        .unwrap_or(last_index)
        .min(last_index);
    micros_to_ms(sorted_micros[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_timing_stats_capture_average_and_percentiles() {
        let mut stats = RollingTimingStats::default();
        stats.record_micros(1_000);
        stats.record_micros(2_000);
        stats.record_micros(8_000);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.sample_count, 3);
        assert_eq!(snapshot.last_ms, 8.0);
        assert_eq!(snapshot.max_ms, 8.0);
        assert!(snapshot.avg_ms >= 3.6 && snapshot.avg_ms <= 3.7);
        assert_eq!(snapshot.p50_ms, 2.0);
        assert_eq!(snapshot.p95_ms, 8.0);
    }
}
