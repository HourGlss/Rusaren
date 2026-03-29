use game_domain::{LobbyId, MatchId, PlayerId};
use game_net::ServerControlEvent;

use super::{fill_random, AppTransport, ConnectionId, PlayerLocation, ServerApp};
use crate::diagnostics::OutboundPacketKind;

impl ServerApp {
    pub(super) fn send_error<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        message: &str,
    ) {
        self.send_event(
            transport,
            player_id,
            ServerControlEvent::Error {
                message: message.to_string(),
            },
        );
    }

    pub(super) fn send_direct_error<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        message: &str,
    ) {
        self.send_direct_event(
            transport,
            connection_id,
            0,
            ServerControlEvent::Error {
                message: message.to_string(),
            },
        );
    }

    pub(super) fn send_event<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        event: ServerControlEvent,
    ) {
        let Some(connection_id) = self.player_connections.get(&player_id).copied() else {
            return;
        };
        let seq = match self.players.get_mut(&player_id) {
            Some(player) => player.next_outbound_seq(),
            None => 0,
        };
        self.send_direct_event(transport, connection_id, seq, event);
    }

    pub(super) fn send_direct_event<T: AppTransport>(
        &mut self,
        transport: &mut T,
        connection_id: ConnectionId,
        seq: u32,
        event: ServerControlEvent,
    ) {
        let packet_kind = OutboundPacketKind::from_event(&event);
        let packet = match event.encode_packet(seq, self.clock_seconds) {
            Ok(packet) => packet,
            Err(_) => return,
        };
        self.diagnostics
            .record_outbound_packet(packet_kind, packet.len());
        transport.send_to_client(connection_id, packet);
    }

    pub(super) fn broadcast_event<T: AppTransport>(
        &mut self,
        transport: &mut T,
        recipients: &[PlayerId],
        event: ServerControlEvent,
    ) {
        for recipient in recipients {
            self.send_event(transport, *recipient, event.clone());
        }
    }

    pub(super) fn ensure_location<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
        expected: PlayerLocation,
    ) -> bool {
        match self.players.get(&player_id) {
            Some(player) if player.location == expected => true,
            Some(_) => {
                self.send_error(
                    transport,
                    player_id,
                    "player is in the wrong state for that command",
                );
                false
            }
            None => {
                self.send_error(transport, player_id, "player is not connected");
                false
            }
        }
    }

    pub(super) fn require_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<LobbyId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::GameLobby(lobby_id) => Some(lobby_id),
                _ => {
                    self.send_error(transport, player_id, "player is not inside a game lobby");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    pub(super) fn require_match<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<MatchId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::Match(match_id) => Some(match_id),
                _ => {
                    self.send_error(transport, player_id, "player is not inside an active match");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    pub(super) fn require_training<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<MatchId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::Training(training_id) => Some(training_id),
                _ => {
                    self.send_error(transport, player_id, "player is not inside training");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    pub(super) fn require_results<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) -> Option<MatchId> {
        match self.players.get(&player_id) {
            Some(player) => match player.location {
                PlayerLocation::Results(match_id) => Some(match_id),
                _ => {
                    self.send_error(transport, player_id, "player is not on the results screen");
                    None
                }
            },
            None => {
                self.send_error(transport, player_id, "player is not connected");
                None
            }
        }
    }

    pub(super) fn lobby_members(&self, lobby_id: LobbyId) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter_map(|(player_id, player)| match player.location {
                PlayerLocation::GameLobby(current) if current == lobby_id => Some(*player_id),
                _ => None,
            })
            .collect()
    }

    pub(super) fn central_lobby_players(&self) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter_map(|(player_id, player)| match player.location {
                PlayerLocation::CentralLobby => Some(*player_id),
                _ => None,
            })
            .collect()
    }

    pub(super) fn match_recipients(&self, match_id: MatchId) -> Vec<PlayerId> {
        self.matches
            .get(&match_id)
            .map(|runtime| {
                runtime
                    .participants
                    .iter()
                    .copied()
                    .filter(|player_id| self.players.contains_key(player_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn training_recipients(&self, training_id: MatchId) -> Vec<PlayerId> {
        self.training_sessions
            .get(&training_id)
            .map(|runtime| {
                self.players
                    .contains_key(&runtime.participant.player_id)
                    .then_some(vec![runtime.participant.player_id])
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub(super) fn cleanup_empty_lobby(&mut self, lobby_id: LobbyId) {
        let empty = self
            .game_lobbies
            .get(&lobby_id)
            .is_some_and(|runtime| runtime.lobby.player_count() == 0);
        if empty {
            self.game_lobbies.remove(&lobby_id);
        }
    }

    pub(super) fn cleanup_finished_match(&mut self, match_id: MatchId) {
        let still_present = self.players.values().any(|player| {
            matches!(player.location, PlayerLocation::Match(current) | PlayerLocation::Results(current) if current == match_id)
        });
        if !still_present {
            self.matches.remove(&match_id);
        }
    }

    pub(super) fn cleanup_finished_training(&mut self, training_id: MatchId) {
        let still_present = self
            .players
            .values()
            .any(|player| matches!(player.location, PlayerLocation::Training(current) if current == training_id));
        if !still_present {
            self.training_sessions.remove(&training_id);
        }
    }

    pub(super) fn remove_player_connection(&mut self, player_id: PlayerId) {
        if let Some(connection_id) = self.player_connections.remove(&player_id) {
            self.connections.remove(&connection_id);
        }
    }

    pub(super) fn allocate_lobby_id(&mut self) -> LobbyId {
        let lobby_id = match LobbyId::new(self.next_lobby_id) {
            Ok(lobby_id) => lobby_id,
            Err(error) => panic!("generated lobby id should be valid: {error}"),
        };
        self.next_lobby_id = self.next_lobby_id.saturating_add(1);
        lobby_id
    }

    pub(super) fn allocate_match_id(&mut self) -> MatchId {
        let match_id = match MatchId::new(self.next_match_id) {
            Ok(match_id) => match_id,
            Err(error) => panic!("generated match id should be valid: {error}"),
        };
        self.next_match_id = self.next_match_id.saturating_add(1);
        match_id
    }

    pub(super) fn allocate_player_id(&self) -> Result<PlayerId, String> {
        for _ in 0..64 {
            let mut bytes = [0_u8; 4];
            fill_random(&mut bytes).map_err(|error| {
                format!("failed to allocate a secure player id from the operating system: {error}")
            })?;
            let raw = u32::from_le_bytes(bytes);
            let Ok(player_id) = PlayerId::new(raw) else {
                continue;
            };
            if !self.players.contains_key(&player_id) {
                return Ok(player_id);
            }
        }

        Err(String::from(
            "failed to allocate a unique player id after repeated attempts",
        ))
    }

    /// Returns the player id currently bound to a transport connection, if any.
    #[must_use]
    pub fn player_id_for_connection(&self, connection_id: ConnectionId) -> Option<PlayerId> {
        self.connections.get(&connection_id).copied()
    }

    pub(super) fn decode_axis(value: i16, field: &'static str) -> Result<i8, String> {
        match value {
            -1..=1 => match i8::try_from(value) {
                Ok(value) => Ok(value),
                Err(_) => Err(format!(
                    "{field}={value} is outside the allowed range -1..=1"
                )),
            },
            _ => Err(format!(
                "{field}={value} is outside the allowed range -1..=1"
            )),
        }
    }
}
