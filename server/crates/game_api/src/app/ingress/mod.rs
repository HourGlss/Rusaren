use game_domain::{PlayerId, PlayerName};
use game_net::{ClientControlCommand, SequenceTracker, ServerControlEvent, ValidatedInputFrame};

use super::{AppTransport, ConnectedPlayer, ConnectionId, PlayerLocation, ServerApp};

mod lobby;
mod match_flow;

impl ServerApp {
    pub(super) fn handle_packet<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        packet: &[u8],
    ) {
        let bound_player = self.connections.get(&connection_id).copied();
        if let Ok((header, command)) = ClientControlCommand::decode_packet(packet) {
            self.handle_control_packet(transport, connection_id, bound_player, header.seq, command);
            return;
        }

        match ValidatedInputFrame::decode_packet(packet) {
            Ok((header, frame)) => {
                self.handle_input_packet(transport, connection_id, bound_player, header.seq, frame)
            }
            Err(error) => self.handle_invalid_packet(transport, connection_id, bound_player, error),
        }
    }

    fn handle_control_packet<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        bound_player: Option<PlayerId>,
        seq: u32,
        command: ClientControlCommand,
    ) {
        match bound_player {
            Some(player_id) => {
                if !self.observe_inbound_control_sequence(transport, player_id, seq) {
                    return;
                }

                self.handle_control_command(transport, connection_id, player_id, seq, command);
            }
            None => self.handle_unbound_control_packet(transport, connection_id, seq, command),
        }
    }

    fn handle_input_packet<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        bound_player: Option<PlayerId>,
        seq: u32,
        frame: ValidatedInputFrame,
    ) {
        let Some(player_id) = bound_player else {
            self.send_direct_error(
                transport,
                connection_id,
                "first packet must be a connect command",
            );
            return;
        };
        if !self.observe_inbound_input(transport, player_id, seq, frame.client_input_tick) {
            return;
        }

        self.handle_input_frame(transport, player_id, frame);
    }

    fn handle_invalid_packet<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        bound_player: Option<PlayerId>,
        error: game_net::PacketError,
    ) {
        match bound_player {
            Some(player_id) => self.send_error(transport, player_id, &error.to_string()),
            None => self.send_direct_error(transport, connection_id, &error.to_string()),
        }
    }

    fn handle_unbound_control_packet<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        seq: u32,
        command: ClientControlCommand,
    ) {
        match command {
            ClientControlCommand::Connect { player_name } => {
                self.handle_connect_command(transport, connection_id, seq, player_name);
            }
            _ => self.send_direct_error(
                transport,
                connection_id,
                "first packet must be a connect command",
            ),
        }
    }

    fn observe_inbound_control_sequence<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        seq: u32,
    ) -> bool {
        let Some(player) = self.players.get_mut(&player_id) else {
            return true;
        };

        match player.inbound_control.observe(seq) {
            Ok(()) => true,
            Err(error) => {
                self.send_error(transport, player_id, &error.to_string());
                false
            }
        }
    }

    fn observe_inbound_input<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        seq: u32,
        client_input_tick: u32,
    ) -> bool {
        let Some(player) = self.players.get_mut(&player_id) else {
            return true;
        };

        if let Err(error) = player.inbound_input.observe(seq) {
            self.send_error(transport, player_id, &error.to_string());
            return false;
        }
        if let Err(error) = player.observe_client_input_tick(client_input_tick) {
            self.send_error(transport, player_id, &error);
            return false;
        }

        true
    }

    pub(super) fn handle_control_command<T: AppTransport>(
        &mut self,
        transport: &mut T,
        _connection_id: ConnectionId,
        sender_id: PlayerId,
        _seq: u32,
        command: ClientControlCommand,
    ) {
        match command {
            ClientControlCommand::Connect { .. } => {
                self.send_error(transport, sender_id, "player is already connected");
            }
            ClientControlCommand::CreateGameLobby => {
                self.handle_create_game_lobby(transport, sender_id)
            }
            ClientControlCommand::JoinGameLobby { lobby_id } => {
                self.handle_join_game_lobby(transport, sender_id, lobby_id);
            }
            ClientControlCommand::LeaveGameLobby => {
                self.handle_leave_game_lobby(transport, sender_id);
            }
            ClientControlCommand::SelectTeam { team } => {
                self.handle_select_team(transport, sender_id, team);
            }
            ClientControlCommand::SetReady { ready } => {
                self.handle_set_ready(transport, sender_id, ready);
            }
            ClientControlCommand::ChooseSkill { tree, tier } => {
                self.handle_choose_skill(transport, sender_id, tree, tier);
            }
            ClientControlCommand::QuitToCentralLobby => {
                self.handle_quit_to_central_lobby(transport, sender_id);
            }
        }
    }

    pub(super) fn handle_connect_command<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        seq: u32,
        player_name: PlayerName,
    ) {
        if self.connections.contains_key(&connection_id) {
            self.send_direct_error(transport, connection_id, "connection is already bound");
            return;
        }

        let player_id = match self.allocate_player_id() {
            Ok(player_id) => player_id,
            Err(message) => {
                self.send_direct_error(transport, connection_id, &message);
                return;
            }
        };
        let record = match self.record_store.load_or_create(&player_name) {
            Ok(record) => record,
            Err(error) => {
                self.send_direct_error(transport, connection_id, &error.to_string());
                return;
            }
        };

        let mut inbound_control = SequenceTracker::new();
        if let Err(error) = inbound_control.observe(seq) {
            self.send_direct_error(transport, connection_id, &error.to_string());
            return;
        }

        self.connections.insert(connection_id, player_id);
        self.player_connections.insert(player_id, connection_id);
        self.players.insert(
            player_id,
            ConnectedPlayer {
                player_name: player_name.clone(),
                record,
                location: PlayerLocation::CentralLobby,
                inbound_control,
                inbound_input: SequenceTracker::new(),
                newest_client_input_tick: None,
                next_outbound_seq: 0,
            },
        );

        self.send_event(
            transport,
            player_id,
            ServerControlEvent::Connected {
                player_id,
                player_name,
                record,
                skill_catalog: Self::build_skill_catalog(&self.content),
            },
        );
        self.send_lobby_directory_snapshot(transport, player_id);
    }

}
