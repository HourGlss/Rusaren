use std::collections::BTreeMap;

use game_content::{EffectPayload, GameContent, SkillBehavior, SkillDefinition};
use game_domain::{PlayerId, RoundNumber, TeamAssignment, TeamSide};
use game_match::MatchSession;
use game_net::{
    ArenaCombatTextEntry, ArenaCombatTextStyle, CombatSummaryLine, MatchSummarySnapshot,
    RoundSummarySnapshot,
};
use game_sim::SimulationWorld;

use crate::combat_log::{
    CombatLogCastCancelReason, CombatLogEntry, CombatLogEvent, CombatLogStatusRemovedReason,
    CombatLogTargetKind,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SummaryTotals {
    damage_done: u32,
    healing_to_allies: u32,
    healing_to_enemies: u32,
    cc_used: u16,
    cc_hits: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlayerCombatRecordTotals {
    pub damage_done: u32,
    pub healing_done: u32,
    pub cc_used: u16,
    pub cc_hits: u16,
}

#[derive(Debug, Default)]
pub(crate) struct MatchCombatFeedback {
    round_totals: BTreeMap<PlayerId, SummaryTotals>,
    running_totals: BTreeMap<PlayerId, SummaryTotals>,
    pending_text: BTreeMap<PlayerId, Vec<ArenaCombatTextEntry>>,
}

impl MatchCombatFeedback {
    pub(crate) fn observe_entry(
        &mut self,
        content: &GameContent,
        roster: &[TeamAssignment],
        session: &MatchSession,
        world: &SimulationWorld,
        entry: &CombatLogEntry,
    ) {
        match &entry.event {
            CombatLogEvent::CastStarted {
                player_id, slot, ..
            } => self.observe_cast_started(content, session, *player_id, *slot),
            CombatLogEvent::DamageApplied {
                source_player_id,
                target_kind,
                target_id,
                amount,
                ..
            } => self.observe_damage_applied(
                world,
                *source_player_id,
                *target_kind,
                *target_id,
                *amount,
            ),
            CombatLogEvent::HealingApplied {
                source_player_id,
                target_player_id,
                amount,
                ..
            } => self.observe_healing_applied(
                roster,
                world,
                *source_player_id,
                *target_player_id,
                *amount,
            ),
            CombatLogEvent::StatusApplied {
                source_player_id,
                target_player_id,
                status_kind,
                stacks,
                ..
            } => self.observe_status_applied(
                roster,
                world,
                *source_player_id,
                *target_player_id,
                status_kind,
                *stacks,
            ),
            CombatLogEvent::StatusRemoved {
                target_player_id,
                status_kind,
                reason,
                ..
            } => self.observe_status_removed(world, *target_player_id, status_kind, *reason),
            CombatLogEvent::DispelResult {
                source_player_id,
                target_player_id,
                removed_statuses,
                ..
            } => self.observe_dispel_result(
                world,
                *source_player_id,
                *target_player_id,
                removed_statuses.len(),
            ),
            CombatLogEvent::DispelCast {
                source_player_id, ..
            } => self.observe_dispel_cast(world, *source_player_id),
            CombatLogEvent::TriggerResolved {
                source_player_id,
                target_kind,
                target_id,
                payload_kind,
                amount,
                ..
            } => self.observe_trigger_resolved(
                world,
                *source_player_id,
                *target_kind,
                *target_id,
                payload_kind,
                *amount,
            ),
            CombatLogEvent::ImpactMiss {
                source_player_id, ..
            } => self.observe_impact_miss(world, *source_player_id),
            CombatLogEvent::CastCanceled {
                player_id, reason, ..
            } => self.observe_cast_canceled(world, *player_id, *reason),
            _ => {}
        }
    }

    fn observe_cast_started(
        &mut self,
        content: &GameContent,
        session: &MatchSession,
        player_id: u32,
        slot: u8,
    ) {
        let Some(source) = parse_player_id(player_id) else {
            return;
        };
        if slot == 0 || !skill_is_cc_capable(content, session, source, slot) {
            return;
        }
        self.bump_cc_used(source);
    }

    fn observe_damage_applied(
        &mut self,
        world: &SimulationWorld,
        source_player_id: u32,
        target_kind: CombatLogTargetKind,
        target_id: u32,
        amount: u16,
    ) {
        let Some(source) = parse_player_id(source_player_id) else {
            return;
        };
        self.bump_damage(source, amount);
        let Some(target_position) = target_position(world, target_kind, target_id) else {
            return;
        };
        self.push_text(
            source,
            target_position,
            ArenaCombatTextStyle::DamageOutgoing,
            amount.to_string(),
        );
        if target_kind != CombatLogTargetKind::Player {
            return;
        }
        if let Some(target_player) = parse_player_id(target_id) {
            self.push_text(
                target_player,
                target_position,
                ArenaCombatTextStyle::DamageIncoming,
                format!("-{amount}"),
            );
        }
    }

    fn observe_healing_applied(
        &mut self,
        roster: &[TeamAssignment],
        world: &SimulationWorld,
        source_player_id: u32,
        target_player_id: u32,
        amount: u16,
    ) {
        let (Some(source), Some(target)) = (
            parse_player_id(source_player_id),
            parse_player_id(target_player_id),
        ) else {
            return;
        };
        let Some(target_position) = player_position(world, target) else {
            return;
        };
        if same_team(roster, source, target) {
            self.bump_healing_to_allies(source, amount);
        } else {
            self.bump_healing_to_enemies(source, amount);
        }
        if source != target {
            self.push_text(
                source,
                target_position,
                ArenaCombatTextStyle::HealOutgoing,
                format!("+{amount}"),
            );
        }
        self.push_text(
            target,
            target_position,
            ArenaCombatTextStyle::HealIncoming,
            format!("+{amount}"),
        );
    }

    fn observe_status_applied(
        &mut self,
        roster: &[TeamAssignment],
        world: &SimulationWorld,
        source_player_id: u32,
        target_player_id: u32,
        status_kind: &str,
        stacks: u8,
    ) {
        if status_kind == "stealth" {
            return;
        }
        let (Some(source), Some(target)) = (
            parse_player_id(source_player_id),
            parse_player_id(target_player_id),
        ) else {
            return;
        };
        let Some(target_position) = player_position(world, target) else {
            return;
        };
        let positive = is_positive_status(status_kind);
        if !positive && is_control_status(status_kind) && !same_team(roster, source, target) {
            self.bump_cc_hits(source);
        }
        let style = if positive {
            ArenaCombatTextStyle::PositiveStatus
        } else {
            ArenaCombatTextStyle::NegativeStatus
        };
        let label = status_text(status_kind, stacks);
        if source != target {
            self.push_text(source, target_position, style, label.clone());
        }
        self.push_text(target, target_position, style, label);
    }

    fn observe_status_removed(
        &mut self,
        world: &SimulationWorld,
        target_player_id: u32,
        status_kind: &str,
        reason: CombatLogStatusRemovedReason,
    ) {
        if status_kind == "stealth" {
            return;
        }
        let Some(target) = parse_player_id(target_player_id) else {
            return;
        };
        let Some(target_position) = player_position(world, target) else {
            return;
        };
        let style = if is_positive_status(status_kind) {
            ArenaCombatTextStyle::PositiveStatus
        } else {
            ArenaCombatTextStyle::NegativeStatus
        };
        let text = match reason {
            CombatLogStatusRemovedReason::Dispelled => {
                format!("{} dispelled", title_case(status_kind))
            }
            CombatLogStatusRemovedReason::DamageBroken => {
                format!("{} broken", title_case(status_kind))
            }
            CombatLogStatusRemovedReason::ShieldConsumed => String::from("Shield spent"),
            CombatLogStatusRemovedReason::Expired => {
                format!("{} faded", title_case(status_kind))
            }
            CombatLogStatusRemovedReason::Defeat => String::from("Cleared on defeat"),
        };
        self.push_text(target, target_position, style, text);
    }

    fn observe_dispel_result(
        &mut self,
        world: &SimulationWorld,
        source_player_id: u32,
        target_player_id: u32,
        removed_status_count: usize,
    ) {
        if removed_status_count == 0 {
            return;
        }
        let (Some(source), Some(target)) = (
            parse_player_id(source_player_id),
            parse_player_id(target_player_id),
        ) else {
            return;
        };
        let Some(target_position) = player_position(world, target) else {
            return;
        };
        self.push_text(
            source,
            target_position,
            ArenaCombatTextStyle::Utility,
            format!("Dispel x{removed_status_count}"),
        );
        self.push_text(
            target,
            target_position,
            ArenaCombatTextStyle::NegativeStatus,
            String::from("Dispelled"),
        );
    }

    fn observe_dispel_cast(&mut self, world: &SimulationWorld, source_player_id: u32) {
        let Some(source) = parse_player_id(source_player_id) else {
            return;
        };
        let Some(position) = player_position(world, source) else {
            return;
        };
        self.push_text(
            source,
            position,
            ArenaCombatTextStyle::Utility,
            String::from("Dispel"),
        );
    }

    fn observe_trigger_resolved(
        &mut self,
        world: &SimulationWorld,
        source_player_id: u32,
        target_kind: CombatLogTargetKind,
        target_id: u32,
        payload_kind: &str,
        amount: u16,
    ) {
        let Some(source) = parse_player_id(source_player_id) else {
            return;
        };
        let Some(position) = target_position(world, target_kind, target_id) else {
            return;
        };
        let (style, text) = if payload_kind == "heal" {
            (
                ArenaCombatTextStyle::HealOutgoing,
                format!("Bloom +{amount}"),
            )
        } else {
            (ArenaCombatTextStyle::Utility, format!("Bloom -{amount}"))
        };
        self.push_text(source, position, style, text.clone());
        if target_kind != CombatLogTargetKind::Player {
            return;
        }
        if let Some(target) = parse_player_id(target_id) {
            let target_style = if payload_kind == "heal" {
                ArenaCombatTextStyle::HealIncoming
            } else {
                ArenaCombatTextStyle::DamageIncoming
            };
            self.push_text(target, position, target_style, text);
        }
    }

    fn observe_impact_miss(&mut self, world: &SimulationWorld, source_player_id: u32) {
        let Some(source) = parse_player_id(source_player_id) else {
            return;
        };
        let Some(position) = player_position(world, source) else {
            return;
        };
        self.push_text(
            source,
            position,
            ArenaCombatTextStyle::Utility,
            String::from("Miss"),
        );
    }

    fn observe_cast_canceled(
        &mut self,
        world: &SimulationWorld,
        player_id: u32,
        reason: CombatLogCastCancelReason,
    ) {
        let Some(player_id) = parse_player_id(player_id) else {
            return;
        };
        let Some(position) = player_position(world, player_id) else {
            return;
        };
        let text = match reason {
            CombatLogCastCancelReason::Interrupt => "Interrupted",
            CombatLogCastCancelReason::ControlLoss => "Control lost",
            CombatLogCastCancelReason::Movement => "Cast canceled",
            CombatLogCastCancelReason::Manual => "Canceled",
            CombatLogCastCancelReason::Defeat => "Defeated",
        };
        self.push_text(
            player_id,
            position,
            ArenaCombatTextStyle::Utility,
            text.to_string(),
        );
    }

    pub(crate) fn finalize_round(
        &mut self,
        roster: &[TeamAssignment],
        round: RoundNumber,
    ) -> RoundSummarySnapshot {
        let snapshot = RoundSummarySnapshot {
            round,
            round_totals: summary_lines(roster, &self.round_totals),
            running_totals: summary_lines(roster, &self.running_totals),
        };
        self.round_totals.clear();
        snapshot
    }

    pub(crate) fn build_match_summary(
        &self,
        roster: &[TeamAssignment],
        rounds_played: u8,
    ) -> MatchSummarySnapshot {
        MatchSummarySnapshot {
            rounds_played,
            totals: summary_lines(roster, &self.running_totals),
        }
    }

    pub(crate) fn player_record_totals(&self, player_id: PlayerId) -> PlayerCombatRecordTotals {
        let totals = self
            .running_totals
            .get(&player_id)
            .copied()
            .unwrap_or_default();
        PlayerCombatRecordTotals {
            damage_done: totals.damage_done,
            healing_done: totals.healing_to_allies,
            cc_used: totals.cc_used,
            cc_hits: totals.cc_hits,
        }
    }

    pub(crate) fn drain_pending_text(&mut self) -> Vec<(PlayerId, Vec<ArenaCombatTextEntry>)> {
        std::mem::take(&mut self.pending_text).into_iter().collect()
    }

    fn bump_damage(&mut self, player_id: PlayerId, amount: u16) {
        let amount = u32::from(amount);
        let round = self.round_totals.entry(player_id).or_default();
        round.damage_done = round.damage_done.saturating_add(amount);
        let running = self.running_totals.entry(player_id).or_default();
        running.damage_done = running.damage_done.saturating_add(amount);
    }

    fn bump_healing_to_allies(&mut self, player_id: PlayerId, amount: u16) {
        self.bump_healing(player_id, amount, true);
    }

    fn bump_healing_to_enemies(&mut self, player_id: PlayerId, amount: u16) {
        self.bump_healing(player_id, amount, false);
    }

    fn bump_healing(&mut self, player_id: PlayerId, amount: u16, allied: bool) {
        let amount = u32::from(amount);
        if allied {
            let round = self.round_totals.entry(player_id).or_default();
            round.healing_to_allies = round.healing_to_allies.saturating_add(amount);
            let running = self.running_totals.entry(player_id).or_default();
            running.healing_to_allies = running.healing_to_allies.saturating_add(amount);
        } else {
            let round = self.round_totals.entry(player_id).or_default();
            round.healing_to_enemies = round.healing_to_enemies.saturating_add(amount);
            let running = self.running_totals.entry(player_id).or_default();
            running.healing_to_enemies = running.healing_to_enemies.saturating_add(amount);
        }
    }

    fn bump_cc_used(&mut self, player_id: PlayerId) {
        let round = self.round_totals.entry(player_id).or_default();
        round.cc_used = round.cc_used.saturating_add(1);
        let running = self.running_totals.entry(player_id).or_default();
        running.cc_used = running.cc_used.saturating_add(1);
    }

    fn bump_cc_hits(&mut self, player_id: PlayerId) {
        let round = self.round_totals.entry(player_id).or_default();
        round.cc_hits = round.cc_hits.saturating_add(1);
        let running = self.running_totals.entry(player_id).or_default();
        running.cc_hits = running.cc_hits.saturating_add(1);
    }

    fn push_text(
        &mut self,
        recipient: PlayerId,
        position: (i16, i16),
        style: ArenaCombatTextStyle,
        text: String,
    ) {
        if text.is_empty() {
            return;
        }
        self.pending_text
            .entry(recipient)
            .or_default()
            .push(ArenaCombatTextEntry {
                x: position.0,
                y: position.1,
                style,
                text,
            });
    }
}

fn parse_player_id(raw: u32) -> Option<PlayerId> {
    PlayerId::new(raw).ok()
}

fn same_team(roster: &[TeamAssignment], left: PlayerId, right: PlayerId) -> bool {
    match (
        team_for_player(roster, left),
        team_for_player(roster, right),
    ) {
        (Some(left_team), Some(right_team)) => left_team == right_team,
        _ => false,
    }
}

fn team_for_player(roster: &[TeamAssignment], player_id: PlayerId) -> Option<TeamSide> {
    roster
        .iter()
        .find(|assignment| assignment.player_id == player_id)
        .map(|assignment| assignment.team)
}

fn player_position(world: &SimulationWorld, player_id: PlayerId) -> Option<(i16, i16)> {
    world
        .player_state(player_id)
        .map(|state| (state.x, state.y))
}

fn target_position(
    world: &SimulationWorld,
    target_kind: CombatLogTargetKind,
    target_id: u32,
) -> Option<(i16, i16)> {
    match target_kind {
        CombatLogTargetKind::Player => {
            parse_player_id(target_id).and_then(|id| player_position(world, id))
        }
        CombatLogTargetKind::Deployable => world
            .deployables()
            .into_iter()
            .find(|deployable| deployable.id == target_id)
            .map(|deployable| (deployable.x, deployable.y)),
    }
}

fn summary_lines(
    roster: &[TeamAssignment],
    totals_by_player: &BTreeMap<PlayerId, SummaryTotals>,
) -> Vec<CombatSummaryLine> {
    roster
        .iter()
        .map(|assignment| {
            let totals = totals_by_player
                .get(&assignment.player_id)
                .copied()
                .unwrap_or_default();
            CombatSummaryLine {
                player_id: assignment.player_id,
                player_name: assignment.player_name.clone(),
                team: assignment.team,
                damage_done: totals.damage_done,
                healing_to_allies: totals.healing_to_allies,
                healing_to_enemies: totals.healing_to_enemies,
                cc_used: totals.cc_used,
                cc_hits: totals.cc_hits,
            }
        })
        .collect()
}

fn skill_is_cc_capable(
    content: &GameContent,
    session: &MatchSession,
    player_id: PlayerId,
    slot: u8,
) -> bool {
    let Some(choice) = session.equipped_choice(player_id, slot) else {
        return false;
    };
    let Some(skill) = content.skills().resolve(&choice) else {
        return false;
    };
    skill_payload(skill).is_some_and(payload_applies_crowd_control)
}

fn skill_payload(skill: &SkillDefinition) -> Option<&EffectPayload> {
    match &skill.behavior {
        SkillBehavior::Projectile { payload, .. }
        | SkillBehavior::Beam { payload, .. }
        | SkillBehavior::Burst { payload, .. }
        | SkillBehavior::Nova { payload, .. }
        | SkillBehavior::Channel { payload, .. }
        | SkillBehavior::Summon { payload, .. }
        | SkillBehavior::Trap { payload, .. }
        | SkillBehavior::Aura { payload, .. } => Some(payload),
        SkillBehavior::Dash { payload, .. } => payload.as_ref(),
        SkillBehavior::Teleport { .. }
        | SkillBehavior::Passive { .. }
        | SkillBehavior::Ward { .. }
        | SkillBehavior::Barrier { .. } => None,
    }
}

fn payload_applies_crowd_control(payload: &EffectPayload) -> bool {
    payload
        .status
        .as_ref()
        .is_some_and(|status| is_control_status(status_label(status.kind)))
}

fn status_label(kind: game_content::StatusKind) -> &'static str {
    match kind {
        game_content::StatusKind::Poison => "poison",
        game_content::StatusKind::Hot => "hot",
        game_content::StatusKind::Chill => "chill",
        game_content::StatusKind::Root => "root",
        game_content::StatusKind::Haste => "haste",
        game_content::StatusKind::Silence => "silence",
        game_content::StatusKind::Stun => "stun",
        game_content::StatusKind::Sleep => "sleep",
        game_content::StatusKind::Shield => "shield",
        game_content::StatusKind::Stealth => "stealth",
        game_content::StatusKind::Reveal => "reveal",
        game_content::StatusKind::Fear => "fear",
    }
}

fn is_positive_status(status_kind: &str) -> bool {
    matches!(status_kind, "hot" | "haste" | "shield" | "stealth")
}

fn is_control_status(status_kind: &str) -> bool {
    matches!(
        status_kind,
        "chill" | "root" | "silence" | "stun" | "sleep" | "fear"
    )
}

fn status_text(status_kind: &str, stacks: u8) -> String {
    if stacks > 1 {
        format!("{} x{stacks}", title_case(status_kind))
    } else {
        title_case(status_kind)
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}
