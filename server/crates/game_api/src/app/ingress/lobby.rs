use game_content::generate_template_match_map;
use game_domain::{LobbyId, PlayerId, ReadyState, TeamSide};
use game_lobby::{Lobby, LobbyEvent, LobbyPhase};
use game_net::ServerControlEvent;

use super::super::{fill_random, GameLobbyRuntime, PlayerLocation, ServerApp};
use super::AppTransport;

impl ServerApp {
    pub(in super::super) fn handle_create_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        if !self.ensure_location(transport, sender_id, PlayerLocation::CentralLobby) {
            return;
        }

        let lobby_id = self.allocate_lobby_id();
        let mut lobby = Lobby::new(lobby_id);
        let (player_name, record) = match self.players.get(&sender_id) {
            Some(player) => (player.player_name.clone(), player.record.clone()),
            None => {
                self.send_error(transport, sender_id, "player is not connected");
                return;
            }
        };

        if let Err(error) = lobby.add_player(sender_id, player_name, record) {
            self.send_error(transport, sender_id, &error.to_string());
            return;
        }

        let Some(template_map) = self.content.map_by_id("template_arena") else {
            self.send_error(
                transport,
                sender_id,
                "template_arena.txt must exist before creating a game lobby",
            );
            return;
        };
        let mut seed_bytes = [0_u8; 8];
        let seed = if fill_random(&mut seed_bytes).is_ok() {
            u64::from_le_bytes(seed_bytes)
        } else {
            u64::from(lobby_id.get()).wrapping_mul(0x9E37_79B9)
        };
        let map = match generate_template_match_map(
            template_map,
            format!("lobby_{}_arena", lobby_id.get()),
            seed,
        ) {
            Ok(map) => map,
            Err(error) => {
                self.send_error(transport, sender_id, &error.to_string());
                return;
            }
        };

        self.game_lobbies
            .insert(lobby_id, GameLobbyRuntime { lobby, map });
        if let Some(player) = self.players.get_mut(&sender_id) {
            player.location = PlayerLocation::GameLobby(lobby_id);
        }

        self.send_event(
            transport,
            sender_id,
            ServerControlEvent::GameLobbyCreated { lobby_id },
        );
        self.send_event(
            transport,
            sender_id,
            ServerControlEvent::GameLobbyJoined {
                lobby_id,
                player_id: sender_id,
            },
        );
        self.send_game_lobby_snapshot(transport, lobby_id, sender_id);
        self.broadcast_lobby_directory_snapshot(transport);
    }

    pub(in super::super) fn handle_join_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        lobby_id: LobbyId,
    ) {
        if !self.ensure_location(transport, sender_id, PlayerLocation::CentralLobby) {
            return;
        }

        let (player_name, record) = match self.players.get(&sender_id) {
            Some(player) => (player.player_name.clone(), player.record.clone()),
            None => {
                self.send_error(transport, sender_id, "player is not connected");
                return;
            }
        };

        let lobby = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => &mut runtime.lobby,
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        if let Err(error) = lobby.add_player(sender_id, player_name, record) {
            self.send_error(transport, sender_id, &error.to_string());
            return;
        }

        if let Some(player) = self.players.get_mut(&sender_id) {
            player.location = PlayerLocation::GameLobby(lobby_id);
        }

        let recipients = self.lobby_members(lobby_id);
        self.broadcast_event(
            transport,
            &recipients,
            ServerControlEvent::GameLobbyJoined {
                lobby_id,
                player_id: sender_id,
            },
        );
        self.broadcast_game_lobby_snapshot(transport, lobby_id);
        self.broadcast_lobby_directory_snapshot(transport);
    }

    pub(in super::super) fn handle_leave_game_lobby<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
    ) {
        let lobby_id = match self.require_game_lobby(transport, sender_id) {
            Some(lobby_id) => lobby_id,
            None => return,
        };

        let lobby_phase = match self.game_lobbies.get(&lobby_id) {
            Some(runtime) => runtime.lobby.phase().clone(),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };
        if !matches!(lobby_phase, LobbyPhase::Open) {
            self.send_error(
                transport,
                sender_id,
                "players cannot leave after countdown starts",
            );
            return;
        }

        let event = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => runtime.lobby.leave_or_disconnect_player(sender_id),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        match event {
            Ok(LobbyEvent::PlayerLeft { .. }) => {
                if let Some(player) = self.players.get_mut(&sender_id) {
                    player.location = PlayerLocation::CentralLobby;
                }
                let recipients = self.lobby_members(lobby_id);
                self.broadcast_event(
                    transport,
                    &recipients,
                    ServerControlEvent::GameLobbyLeft {
                        lobby_id,
                        player_id: sender_id,
                    },
                );
                let record = self
                    .players
                    .get(&sender_id)
                    .map(|player| player.record.clone())
                    .unwrap_or_default();
                self.send_event(
                    transport,
                    sender_id,
                    ServerControlEvent::ReturnedToCentralLobby { record },
                );
                self.cleanup_empty_lobby(lobby_id);
                self.broadcast_game_lobby_snapshot(transport, lobby_id);
                self.send_lobby_directory_snapshot(transport, sender_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            Ok(other) => self.send_error(
                transport,
                sender_id,
                &format!("unexpected leave event: {other:?}"),
            ),
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }

    pub(in super::super) fn handle_select_team<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        team: TeamSide,
    ) {
        let lobby_id = match self.require_game_lobby(transport, sender_id) {
            Some(lobby_id) => lobby_id,
            None => return,
        };

        let events = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => runtime.lobby.select_team(sender_id, team),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        match events {
            Ok(events) => {
                self.broadcast_lobby_events(transport, lobby_id, &events);
                self.broadcast_game_lobby_snapshot(transport, lobby_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }

    pub(in super::super) fn handle_set_ready<T: AppTransport>(
        &mut self,
        transport: &mut T,
        sender_id: PlayerId,
        ready: ReadyState,
    ) {
        let lobby_id = match self.require_game_lobby(transport, sender_id) {
            Some(lobby_id) => lobby_id,
            None => return,
        };

        let events = match self.game_lobbies.get_mut(&lobby_id) {
            Some(runtime) => runtime.lobby.set_ready(sender_id, ready),
            None => {
                self.send_error(transport, sender_id, "game lobby does not exist");
                return;
            }
        };

        match events {
            Ok(events) => {
                self.broadcast_lobby_events(transport, lobby_id, &events);
                self.broadcast_game_lobby_snapshot(transport, lobby_id);
                self.broadcast_lobby_directory_snapshot(transport);
            }
            Err(error) => self.send_error(transport, sender_id, &error.to_string()),
        }
    }
}
