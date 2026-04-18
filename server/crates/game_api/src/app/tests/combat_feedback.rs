use super::*;
use crate::combat_log::{
    CombatLogCastCancelReason, CombatLogEntry, CombatLogEvent, CombatLogPhase,
    CombatLogRemovedStatus, CombatLogStatusRemovedReason, CombatLogTargetKind,
};
use game_net::ArenaCombatTextStyle;

#[test]
#[allow(clippy::too_many_lines)]
fn combat_feedback_tracks_totals_and_combat_text_from_runtime_log_entries() {
    let mut server = ServerApp::new();
    let mut transport = InMemoryTransport::new();
    let (mut alice, mut bob) = connect_pair(&mut server, &mut transport);
    let match_id = enter_combat(
        &mut server,
        &mut transport,
        &mut alice,
        &mut bob,
        skill(SkillTree::Cleric, 1),
        skill(SkillTree::Rogue, 1),
    );
    let content = server.content.clone();
    let alice_id = alice.player_id().expect("alice id");
    let bob_id = bob.player_id().expect("bob id");

    let entry = |event| CombatLogEntry::new(match_id, 1, CombatLogPhase::Combat, 7, event);
    let entries = vec![
        entry(CombatLogEvent::DamageApplied {
            source_player_id: alice_id.get(),
            target_kind: CombatLogTargetKind::Player,
            target_id: bob_id.get(),
            slot: 1,
            amount: 17,
            critical: false,
            remaining_hit_points: 83,
            defeated: false,
            status_kind: None,
            trigger: None,
        }),
        entry(CombatLogEvent::HealingApplied {
            source_player_id: bob_id.get(),
            target_player_id: alice_id.get(),
            slot: 1,
            amount: 9,
            critical: false,
            resulting_hit_points: 92,
            status_kind: None,
            trigger: None,
        }),
        entry(CombatLogEvent::StatusApplied {
            source_player_id: alice_id.get(),
            target_player_id: bob_id.get(),
            slot: 1,
            status_kind: String::from("sleep"),
            stacks: 1,
            stack_delta: 1,
            remaining_ms: 1200,
        }),
        entry(CombatLogEvent::StatusRemoved {
            source_player_id: alice_id.get(),
            target_player_id: bob_id.get(),
            slot: 1,
            status_kind: String::from("sleep"),
            stacks: 1,
            remaining_ms: 300,
            reason: CombatLogStatusRemovedReason::Dispelled,
        }),
        entry(CombatLogEvent::DispelCast {
            source_player_id: bob_id.get(),
            slot: 1,
            scope: String::from("negative"),
            max_statuses: 1,
        }),
        entry(CombatLogEvent::DispelResult {
            source_player_id: bob_id.get(),
            slot: 1,
            target_player_id: alice_id.get(),
            removed_statuses: vec![CombatLogRemovedStatus {
                source_player_id: alice_id.get(),
                slot: 1,
                status_kind: String::from("hot"),
                stacks: 1,
                remaining_ms: 400,
            }],
            triggered_payload_count: 1,
        }),
        entry(CombatLogEvent::TriggerResolved {
            source_player_id: alice_id.get(),
            target_kind: CombatLogTargetKind::Player,
            target_id: bob_id.get(),
            slot: 1,
            status_kind: String::from("hot"),
            trigger: crate::combat_log::CombatLogTriggerReason::Dispel,
            payload_kind: String::from("heal"),
            amount: 12,
        }),
        entry(CombatLogEvent::ImpactMiss {
            source_player_id: alice_id.get(),
            slot: 1,
            reason: crate::combat_log::CombatLogMissReason::Blocked,
        }),
        entry(CombatLogEvent::CastCanceled {
            player_id: bob_id.get(),
            slot: 1,
            reason: CombatLogCastCancelReason::Interrupt,
        }),
    ];

    let runtime = server.matches.get_mut(&match_id).expect("match runtime");
    for current in &entries {
        runtime.feedback.observe_entry(
            &content,
            &runtime.roster,
            &runtime.session,
            &runtime.world,
            current,
        );
    }

    let pending_text = runtime.feedback.drain_pending_text();
    let alice_text = pending_text
        .iter()
        .find_map(|(player_id, entries)| (*player_id == alice_id).then_some(entries))
        .expect("alice should receive combat text");
    let bob_text = pending_text
        .iter()
        .find_map(|(player_id, entries)| (*player_id == bob_id).then_some(entries))
        .expect("bob should receive combat text");

    assert!(alice_text.iter().any(|entry| {
        entry.style == ArenaCombatTextStyle::DamageOutgoing && entry.text == "17"
    }));
    assert!(alice_text
        .iter()
        .any(|entry| { entry.style == ArenaCombatTextStyle::HealIncoming && entry.text == "+9" }));
    assert!(alice_text
        .iter()
        .any(|entry| { entry.style == ArenaCombatTextStyle::Utility && entry.text == "Miss" }));
    assert!(bob_text.iter().any(|entry| {
        entry.style == ArenaCombatTextStyle::DamageIncoming && entry.text == "-17"
    }));
    assert!(bob_text.iter().any(|entry| {
        entry.style == ArenaCombatTextStyle::NegativeStatus
            && (entry.text == "Sleep" || entry.text == "Sleep dispelled")
    }));
    assert!(bob_text.iter().any(|entry| {
        entry.style == ArenaCombatTextStyle::HealIncoming && entry.text == "Bloom +12"
    }));
    assert!(bob_text.iter().any(|entry| {
        entry.style == ArenaCombatTextStyle::Utility && entry.text == "Interrupted"
    }));
    assert!(alice_text.iter().any(|entry| {
        entry.style == ArenaCombatTextStyle::NegativeStatus && entry.text == "Dispelled"
    }));

    let alice_totals = runtime.feedback.player_record_totals(alice_id);
    assert_eq!(alice_totals.damage_done, 17);
    assert_eq!(alice_totals.healing_done, 0);
    assert_eq!(alice_totals.cc_used, 0);
    assert_eq!(alice_totals.cc_hits, 1);

    let bob_totals = runtime.feedback.player_record_totals(bob_id);
    assert_eq!(bob_totals.damage_done, 0);
    assert_eq!(bob_totals.healing_done, 0);
    assert_eq!(bob_totals.cc_hits, 0);

    let round_summary = runtime
        .feedback
        .finalize_round(&runtime.roster, runtime.session.current_round());
    let alice_round = round_summary
        .round_totals
        .iter()
        .find(|line| line.player_id == alice_id)
        .expect("alice round totals");
    let bob_round = round_summary
        .round_totals
        .iter()
        .find(|line| line.player_id == bob_id)
        .expect("bob round totals");
    assert_eq!(alice_round.damage_done, 17);
    assert_eq!(alice_round.cc_hits, 1);
    assert_eq!(bob_round.healing_to_enemies, 9);

    let match_summary = runtime.feedback.build_match_summary(&runtime.roster, 1);
    let alice_match = match_summary
        .totals
        .iter()
        .find(|line| line.player_id == alice_id)
        .expect("alice match totals");
    assert_eq!(alice_match.damage_done, 17);
    assert_eq!(alice_match.cc_hits, 1);
}
