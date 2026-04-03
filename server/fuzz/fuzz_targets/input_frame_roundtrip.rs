#![no_main]

use arbitrary::Arbitrary;
use game_net::{
    ValidatedInputFrame, ALLOWED_BUTTONS_MASK, BUTTON_CANCEL, BUTTON_CAST, BUTTON_PRIMARY,
    BUTTON_QUIT_TO_LOBBY, BUTTON_SECONDARY, BUTTON_SELF_CAST,
};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct FuzzInputFrame {
    client_input_tick: u32,
    move_horizontal_q: i16,
    move_vertical_q: i16,
    aim_horizontal_q: i16,
    aim_vertical_q: i16,
    buttons: u16,
    ability_or_context: u16,
    seq: u32,
    sim_tick: u32,
}

impl FuzzInputFrame {
    fn into_real(self) -> ValidatedInputFrame {
        let mut buttons = self.buttons & ALLOWED_BUTTONS_MASK;
        let allowed_ordered_bits = [
            BUTTON_PRIMARY,
            BUTTON_SECONDARY,
            BUTTON_CAST,
            BUTTON_CANCEL,
            BUTTON_QUIT_TO_LOBBY,
        ];
        if buttons == 0 {
            buttons = allowed_ordered_bits
                [usize::from(self.buttons % (allowed_ordered_bits.len() as u16))];
        }

        if buttons & BUTTON_SELF_CAST != 0 {
            buttons |= BUTTON_CAST;
        }

        let ability_or_context = if buttons & BUTTON_CAST != 0 {
            self.ability_or_context.max(1)
        } else {
            0
        };

        ValidatedInputFrame::new(
            self.client_input_tick,
            self.move_horizontal_q,
            self.move_vertical_q,
            self.aim_horizontal_q,
            self.aim_vertical_q,
            buttons,
            ability_or_context,
        )
        .expect("sanitized fuzz input frame should validate")
    }
}

fuzz_target!(|input: FuzzInputFrame| {
    let seq = input.seq;
    let sim_tick = input.sim_tick;
    let frame = input.into_real();
    let packet = frame
        .encode_packet(seq, sim_tick)
        .expect("valid fuzz input frame should encode");
    let (header, decoded) =
        ValidatedInputFrame::decode_packet(&packet).expect("encoded fuzz input should decode");

    assert_eq!(decoded, frame);
    assert_eq!(header.seq, seq);
    assert_eq!(header.sim_tick, sim_tick);
});
