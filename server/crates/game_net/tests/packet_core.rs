#![allow(clippy::expect_used)]

use std::fmt;

use game_net::{
    ChannelId, PacketError, PacketHeader, PacketKind, SequenceTracker, ValidatedInputFrame,
    ALLOWED_BUTTONS_MASK, BUTTON_CAST, BUTTON_PRIMARY, HEADER_LEN, INPUT_PAYLOAD_LEN,
    INPUT_PAYLOAD_LEN_U16, PACKET_MAGIC, PROTOCOL_VERSION,
};
use proptest::prelude::*;

fn assert_ok<T, E: fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected Ok(..), got Err({error:?})"),
    }
}

#[test]
fn header_round_trips_for_valid_packets() {
    let header = assert_ok(PacketHeader::new(
        ChannelId::Input,
        PacketKind::InputFrame,
        0,
        INPUT_PAYLOAD_LEN_U16,
        7,
        33,
    ));

    let payload = vec![0_u8; INPUT_PAYLOAD_LEN];
    let bytes = header.encode(&payload);
    let (decoded, decoded_payload) = assert_ok(PacketHeader::decode(&bytes));

    assert_eq!(decoded, header);
    assert_eq!(decoded_payload, payload.as_slice());
}

#[test]
fn header_rejects_short_packets_invalid_magic_and_invalid_versions() {
    assert_eq!(
        PacketHeader::decode(&[0_u8; HEADER_LEN - 1]),
        Err(PacketError::PacketTooShort {
            actual: HEADER_LEN - 1,
            minimum: HEADER_LEN,
        })
    );

    let mut packet = vec![0_u8; HEADER_LEN];
    packet[0..2].copy_from_slice(&0x0000_u16.to_le_bytes());
    packet[2] = PROTOCOL_VERSION;
    packet[3] = 1;
    packet[4] = 16;
    assert_eq!(
        PacketHeader::decode(&packet),
        Err(PacketError::MagicMismatch {
            expected: PACKET_MAGIC,
            actual: 0,
        })
    );

    packet[0..2].copy_from_slice(&PACKET_MAGIC.to_le_bytes());
    packet[2] = PROTOCOL_VERSION + 1;
    assert_eq!(
        PacketHeader::decode(&packet),
        Err(PacketError::VersionMismatch {
            expected: PROTOCOL_VERSION,
            actual: PROTOCOL_VERSION + 1,
        })
    );
}

#[test]
fn header_rejects_unknown_channels_unknown_kinds_and_bad_lengths() {
    let mut packet = vec![0_u8; HEADER_LEN];
    packet[0..2].copy_from_slice(&PACKET_MAGIC.to_le_bytes());
    packet[2] = PROTOCOL_VERSION;
    packet[3] = 9;
    assert_eq!(
        PacketHeader::decode(&packet),
        Err(PacketError::UnknownChannel(9))
    );

    packet[3] = 1;
    packet[4] = 99;
    assert_eq!(
        PacketHeader::decode(&packet),
        Err(PacketError::UnknownPacketKind {
            channel: ChannelId::Input,
            raw_kind: 99,
        })
    );

    packet[4] = 16;
    packet[6..8].copy_from_slice(&(1_u16).to_le_bytes());
    assert_eq!(
        PacketHeader::decode(&packet),
        Err(PacketError::PayloadLengthMismatch {
            declared: 1,
            actual: 0,
        })
    );
}

#[test]
fn input_frame_validates_button_masks_and_context_consistency() {
    assert_eq!(
        ValidatedInputFrame::new(1, 0, 0, 0, 0, ALLOWED_BUTTONS_MASK + 1, 0),
        Err(PacketError::UnknownButtonBits {
            provided: ALLOWED_BUTTONS_MASK + 1,
            allowed_mask: ALLOWED_BUTTONS_MASK,
        })
    );

    assert_eq!(
        ValidatedInputFrame::new(1, 0, 0, 0, 0, BUTTON_CAST, 0),
        Err(PacketError::MissingAbilityContext)
    );

    assert_eq!(
        ValidatedInputFrame::new(1, 0, 0, 0, 0, 0, 7),
        Err(PacketError::UnexpectedAbilityContext(7))
    );

    assert_eq!(
        ValidatedInputFrame::new(1, -10, 10, 25, -25, BUTTON_CAST | BUTTON_PRIMARY, 7),
        Ok(ValidatedInputFrame {
            client_input_tick: 1,
            move_horizontal_q: -10,
            move_vertical_q: 10,
            aim_horizontal_q: 25,
            aim_vertical_q: -25,
            buttons: BUTTON_CAST | BUTTON_PRIMARY,
            ability_or_context: 7,
        })
    );
}

#[test]
fn input_frame_packet_round_trips_and_rejects_wrong_packets() {
    let frame = assert_ok(ValidatedInputFrame::new(3, 1, -1, 50, -50, BUTTON_CAST, 9));
    let packet = assert_ok(frame.encode_packet(17, 99));

    let (header, decoded_frame) = assert_ok(ValidatedInputFrame::decode_packet(&packet));
    assert_eq!(header.channel_id, ChannelId::Input);
    assert_eq!(header.packet_kind, PacketKind::InputFrame);
    assert_eq!(decoded_frame, frame);

    let wrong_header = assert_ok(PacketHeader::new(
        ChannelId::Control,
        PacketKind::MatchStarted,
        0,
        INPUT_PAYLOAD_LEN_U16,
        17,
        99,
    ))
    .encode(&[0_u8; INPUT_PAYLOAD_LEN]);

    assert_eq!(
        ValidatedInputFrame::decode_packet(&wrong_header),
        Err(PacketError::UnexpectedPacketKind {
            expected_channel: ChannelId::Input,
            expected_kind: PacketKind::InputFrame,
            actual_channel: ChannelId::Control,
            actual_kind: PacketKind::MatchStarted,
        })
    );
}

#[test]
fn input_frame_decode_rejects_bad_input_payload_lengths() {
    let header = assert_ok(PacketHeader::new(
        ChannelId::Input,
        PacketKind::InputFrame,
        0,
        INPUT_PAYLOAD_LEN_U16 - 1,
        1,
        1,
    ));
    let packet = header.encode(&[0_u8; INPUT_PAYLOAD_LEN - 1]);

    assert_eq!(
        ValidatedInputFrame::decode_packet(&packet),
        Err(PacketError::InputPayloadLengthMismatch {
            expected: INPUT_PAYLOAD_LEN,
            actual: INPUT_PAYLOAD_LEN - 1,
        })
    );
}

#[test]
fn sequence_tracker_accepts_increasing_sequences_and_rejects_stale_values() {
    let mut tracker = SequenceTracker::new();

    assert_eq!(tracker.observe(0), Ok(()));
    assert_eq!(tracker.observe(1), Ok(()));
    assert_eq!(tracker.newest(), Some(1));
    assert_eq!(
        tracker.observe(1),
        Err(PacketError::StaleSequence {
            incoming: 1,
            newest: 1,
        })
    );
    assert_eq!(
        tracker.observe(0),
        Err(PacketError::StaleSequence {
            incoming: 0,
            newest: 1,
        })
    );
}

#[test]
fn packet_error_display_covers_all_formatter_groups() {
    let cases = [
        (
            PacketError::PacketTooShort {
                actual: 1,
                minimum: HEADER_LEN,
            },
            "packet length 1 is below the minimum header length 16",
        ),
        (
            PacketError::MissingAbilityContext,
            "cast packets must provide a non-zero ability_or_context",
        ),
        (
            PacketError::UnexpectedTrailingBytes {
                kind: "ClientControlCommand",
                actual: 2,
            },
            "ClientControlCommand payload contained 2 unexpected trailing bytes",
        ),
        (
            PacketError::InvalidEncodedTeam(9),
            "encoded team 9 is invalid",
        ),
        (
            PacketError::FirstPacketMustBeConnect,
            "the first packet on a network session must be a connect command",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

fn valid_channel_kind() -> impl Strategy<Value = (ChannelId, PacketKind)> {
    prop_oneof![
        Just((ChannelId::Control, PacketKind::ControlSnapshot)),
        Just((ChannelId::Control, PacketKind::ControlDelta)),
        Just((ChannelId::Control, PacketKind::LaunchCountdownStarted)),
        Just((ChannelId::Control, PacketKind::MatchStarted)),
        Just((ChannelId::Control, PacketKind::MatchAborted)),
        Just((ChannelId::Control, PacketKind::MatchStatistics)),
        Just((ChannelId::Control, PacketKind::ControlCommand)),
        Just((ChannelId::Control, PacketKind::ControlEvent)),
        Just((ChannelId::Input, PacketKind::InputFrame)),
        Just((ChannelId::Snapshot, PacketKind::FullSnapshot)),
        Just((ChannelId::Snapshot, PacketKind::DeltaSnapshot)),
        Just((ChannelId::Snapshot, PacketKind::EventBatch)),
    ]
}

proptest! {
    #[test]
    fn prop_packet_headers_round_trip_all_valid_channel_kind_pairs(
        (channel_id, packet_kind) in valid_channel_kind(),
        flags in any::<u8>(),
        payload in proptest::collection::vec(any::<u8>(), 0..64),
        seq in any::<u32>(),
        sim_tick in any::<u32>(),
    ) {
        let header = PacketHeader::new(
            channel_id,
            packet_kind,
            flags,
            u16::try_from(payload.len()).expect("payload length should fit in u16"),
            seq,
            sim_tick,
        )
        .expect("strategy only generates valid channel/kind pairs");

        let bytes = header.encode(&payload);
        let (decoded_header, decoded_payload) = PacketHeader::decode(&bytes).expect("encoded header should decode");

        prop_assert_eq!(decoded_header, header);
        prop_assert_eq!(decoded_payload, payload.as_slice());
    }

    #[test]
    fn prop_sequence_tracker_accepts_strictly_increasing_sequences(
        mut seqs in proptest::collection::vec(any::<u32>(), 1..32)
    ) {
        seqs.sort_unstable();
        seqs.dedup();
        prop_assume!(!seqs.is_empty());

        let mut tracker = SequenceTracker::new();
        for seq in &seqs {
            prop_assert_eq!(tracker.observe(*seq), Ok(()));
        }

        let newest = *seqs.last().expect("deduplicated sequence set should be non-empty");
        prop_assert_eq!(
            tracker.observe(newest),
            Err(PacketError::StaleSequence {
                incoming: newest,
                newest,
            })
        );
    }
}
