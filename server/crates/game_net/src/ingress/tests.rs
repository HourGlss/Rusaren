use super::*;
use game_domain::{PlayerName, ReadyState, TeamSide};

fn player_name(raw: &str) -> PlayerName {
    match PlayerName::new(raw) {
        Ok(player_name) => player_name,
        Err(error) => panic!("valid player name expected: {error}"),
    }
}

#[test]
fn ingress_guard_requires_connect_before_other_packets() {
    let guard = NetworkSessionGuard::new();
    let packet = match (ClientControlCommand::SetReady {
        ready: ReadyState::Ready,
    })
    .encode_packet(1, 0)
    {
        Ok(packet) => packet,
        Err(error) => panic!("packet should encode: {error}"),
    };

    assert_eq!(
        guard.accept_packet(&packet),
        Err(PacketError::FirstPacketMustBeConnect)
    );
}

#[test]
fn ingress_guard_binds_on_connect_and_rejects_rebinding() {
    let mut guard = NetworkSessionGuard::new();
    let connect = match (ClientControlCommand::Connect {
        player_name: player_name("Alice"),
    })
    .encode_packet(1, 0)
    {
        Ok(packet) => packet,
        Err(error) => panic!("packet should encode: {error}"),
    };

    assert_eq!(guard.accept_packet(&connect), Ok(()));
    assert!(!guard.is_bound());
    guard.mark_bound();
    assert!(guard.is_bound());

    let select_team = match (ClientControlCommand::SelectTeam {
        team: TeamSide::TeamA,
    })
    .encode_packet(2, 0)
    {
        Ok(packet) => packet,
        Err(error) => panic!("packet should encode: {error}"),
    };
    assert_eq!(guard.accept_packet(&select_team), Ok(()));

    let reconnect = match (ClientControlCommand::Connect {
        player_name: player_name("Mallory"),
    })
    .encode_packet(3, 0)
    {
        Ok(packet) => packet,
        Err(error) => panic!("packet should encode: {error}"),
    };
    assert_eq!(
        guard.accept_packet(&reconnect),
        Err(PacketError::ConnectCommandAfterBinding)
    );
}

#[test]
fn ingress_guard_rejects_oversized_packets() {
    let guard = NetworkSessionGuard::new();
    let packet = vec![0_u8; MAX_INGRESS_PACKET_BYTES + 1];

    assert_eq!(
        guard.accept_packet(&packet),
        Err(PacketError::IngressPacketTooLarge {
            actual: MAX_INGRESS_PACKET_BYTES + 1,
            maximum: MAX_INGRESS_PACKET_BYTES,
        })
    );
}

#[test]
fn ingress_guard_accepts_packets_at_the_exact_size_limit() {
    let mut guard = NetworkSessionGuard::new();
    guard.mark_bound();
    let packet = vec![0_u8; MAX_INGRESS_PACKET_BYTES];

    assert_eq!(guard.accept_packet(&packet), Ok(()));
}
