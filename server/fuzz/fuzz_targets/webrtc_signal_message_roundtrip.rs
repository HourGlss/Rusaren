#![no_main]

use arbitrary::Arbitrary;
use game_api::{
    decode_client_signal_message, ClientSignalMessage, SignalingIceCandidate,
    SignalingSessionDescription,
};
use libfuzzer_sys::fuzz_target;

const MAX_SDP_BYTES: usize = 96 * 1024;
const MAX_CANDIDATE_BYTES: usize = 4096;
const MAX_MID_BYTES: usize = 64;

#[derive(Arbitrary, Debug)]
enum FuzzSignalMessage {
    Offer {
        sdp: Vec<u8>,
    },
    IceCandidate {
        candidate: Vec<u8>,
        sdp_mid: Option<Vec<u8>>,
        sdp_mline_index: Option<u16>,
    },
    Bye,
}

impl FuzzSignalMessage {
    fn into_real(self) -> ClientSignalMessage {
        match self {
            Self::Offer { sdp } => ClientSignalMessage::SessionDescription {
                description: SignalingSessionDescription {
                    sdp_type: String::from("offer"),
                    sdp: sanitize_ascii(&sdp, MAX_SDP_BYTES, "v=0\r\n"),
                },
            },
            Self::IceCandidate {
                candidate,
                sdp_mid,
                sdp_mline_index,
            } => ClientSignalMessage::IceCandidate {
                candidate: SignalingIceCandidate {
                    candidate: sanitize_ascii(
                        &candidate,
                        MAX_CANDIDATE_BYTES,
                        "candidate:0 1 UDP 2122252543 127.0.0.1 5000 typ host",
                    ),
                    sdp_mid: sdp_mid.map(|value| sanitize_ascii(&value, MAX_MID_BYTES, "0")),
                    sdp_mline_index,
                },
            },
            Self::Bye => ClientSignalMessage::Bye,
        }
    }
}

fn sanitize_ascii(raw: &[u8], max_len: usize, fallback: &str) -> String {
    let bytes = if raw.is_empty() {
        fallback.as_bytes()
    } else {
        raw
    };
    let mut sanitized = String::with_capacity(bytes.len().min(max_len));
    for byte in bytes.iter().copied().take(max_len) {
        if byte.is_ascii_graphic() || byte == b' ' || byte == b'\r' || byte == b'\n' {
            sanitized.push(char::from(byte));
        }
    }

    if sanitized.trim().is_empty() {
        return String::from(fallback);
    }

    sanitized
}

fuzz_target!(|input: FuzzSignalMessage| {
    let message = input.into_real();
    let json = serde_json::to_string(&message).expect("sanitized fuzz signal should encode");
    let decoded =
        decode_client_signal_message(&json).expect("encoded fuzz signal should decode back");
    assert_eq!(decoded, message);
});
