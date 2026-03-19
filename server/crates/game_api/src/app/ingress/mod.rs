use super::*;

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
            match bound_player {
                Some(player_id) => {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        if let Err(error) = player.inbound_control.observe(header.seq) {
                            self.send_error(transport, player_id, &error.to_string());
                            return;
                        }
                    }

                    self.handle_control_command(
                        transport,
                        connection_id,
                        player_id,
                        header.seq,
                        command,
                    );
                }
                None => match command {
                    ClientControlCommand::Connect { player_name } => {
                        self.handle_connect_command(
                            transport,
                            connection_id,
                            header.seq,
                            player_name,
                        );
                    }
                    _ => self.send_direct_error(
                        transport,
                        connection_id,
                        "first packet must be a connect command",
                    ),
                },
            }
            return;
        }

        match ValidatedInputFrame::decode_packet(packet) {
            Ok((header, frame)) => match bound_player {
                Some(player_id) => {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        if let Err(error) = player.inbound_input.observe(header.seq) {
                            self.send_error(transport, player_id, &error.to_string());
                            return;
                        }
                        if let Err(error) =
                            player.observe_client_input_tick(frame.client_input_tick)
                        {
                            self.send_error(transport, player_id, &error);
                            return;
                        }
                    }

                    self.handle_input_frame(transport, player_id, frame);
                }
                None => self.send_direct_error(
                    transport,
                    connection_id,
                    "first packet must be a connect command",
                ),
            },
            Err(error) => match bound_player {
                Some(player_id) => self.send_error(transport, player_id, &error.to_string()),
                None => self.send_direct_error(transport, connection_id, &error.to_string()),
            },
        }
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
