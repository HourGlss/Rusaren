use game_net::{
    ArenaDeltaSnapshot, ArenaEffectSnapshot, ArenaSessionMode, ArenaStateSnapshot,
    LobbyDirectoryEntry, LobbySnapshotPhase, LobbySnapshotPlayer, ServerControlEvent,
};

use super::{
    AppTransport, ArenaMapDefinition, LobbyId, LobbyPhase, MatchId, PlayerId, ServerApp, TeamSide,
};

mod arena;
mod visibility;

impl ServerApp {
    pub(super) fn send_lobby_directory_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        player_id: PlayerId,
    ) {
        let event = ServerControlEvent::LobbyDirectorySnapshot {
            lobbies: self.build_lobby_directory_entries(),
        };
        self.send_event(transport, player_id, event);
    }

    pub(super) fn broadcast_lobby_directory_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
    ) {
        let recipients = self.central_lobby_players();
        if recipients.is_empty() {
            return;
        }

        let event = ServerControlEvent::LobbyDirectorySnapshot {
            lobbies: self.build_lobby_directory_entries(),
        };
        self.broadcast_event(transport, &recipients, event);
    }

    pub(super) fn send_game_lobby_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        lobby_id: LobbyId,
        player_id: PlayerId,
    ) {
        let Some(event) = self.build_game_lobby_snapshot(lobby_id) else {
            return;
        };
        self.send_event(transport, player_id, event);
    }

    pub(super) fn broadcast_game_lobby_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        lobby_id: LobbyId,
    ) {
        let recipients = self.lobby_members(lobby_id);
        if recipients.is_empty() {
            return;
        }

        let Some(event) = self.build_game_lobby_snapshot(lobby_id) else {
            return;
        };
        self.broadcast_event(transport, &recipients, event);
    }

    pub(super) fn build_lobby_directory_entries(&self) -> Vec<LobbyDirectoryEntry> {
        self.game_lobbies
            .iter()
            .map(|(lobby_id, runtime)| {
                let players = runtime.lobby.players();
                let team_a_count = players
                    .iter()
                    .filter(|player| player.team == Some(TeamSide::TeamA))
                    .count();
                let team_b_count = players
                    .iter()
                    .filter(|player| player.team == Some(TeamSide::TeamB))
                    .count();
                let ready_count = players
                    .iter()
                    .filter(|player| player.ready_state.is_ready())
                    .count();

                LobbyDirectoryEntry {
                    lobby_id: *lobby_id,
                    player_count: u16::try_from(players.len()).unwrap_or(u16::MAX),
                    team_a_count: u16::try_from(team_a_count).unwrap_or(u16::MAX),
                    team_b_count: u16::try_from(team_b_count).unwrap_or(u16::MAX),
                    ready_count: u16::try_from(ready_count).unwrap_or(u16::MAX),
                    phase: Self::lobby_snapshot_phase(runtime.lobby.phase()),
                }
            })
            .collect()
    }

    pub(super) fn build_game_lobby_snapshot(
        &self,
        lobby_id: LobbyId,
    ) -> Option<ServerControlEvent> {
        let runtime = self.game_lobbies.get(&lobby_id)?;
        let players = runtime
            .lobby
            .players()
            .into_iter()
            .map(|player| LobbySnapshotPlayer {
                player_id: player.player_id,
                player_name: player.player_name,
                record: player.record,
                team: player.team,
                ready: player.ready_state,
            })
            .collect();

        Some(ServerControlEvent::GameLobbySnapshot {
            lobby_id,
            phase: Self::lobby_snapshot_phase(runtime.lobby.phase()),
            players,
        })
    }

    pub(super) fn build_arena_state_snapshot(
        &mut self,
        match_id: MatchId,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
    ) -> Option<ArenaStateSnapshot> {
        let runtime = self.matches.get_mut(&match_id)?;
        let (visible_tiles, explored_tiles) = Self::build_visibility_masks(
            &runtime.world,
            &mut runtime.explored_tiles,
            viewer_id,
            map,
        )?;
        let obstacles =
            Self::arena_obstacles_snapshot(runtime.world.obstacles(), map, &explored_tiles);
        let deployables =
            Self::arena_deployables_snapshot(&runtime.world, viewer_id, map, &visible_tiles);
        let players = Self::arena_players_snapshot(runtime, viewer_id, map, &visible_tiles);
        let projectiles =
            Self::arena_projectiles_snapshot(&runtime.world, viewer_id, map, &visible_tiles);
        let (phase, phase_seconds_remaining) = Self::arena_match_phase_snapshot(&runtime.session);

        Some(ArenaStateSnapshot {
            mode: ArenaSessionMode::Match,
            phase,
            phase_seconds_remaining,
            width: runtime.world.arena_width_units(),
            height: runtime.world.arena_height_units(),
            tile_units: map.tile_units,
            footprint_tiles: runtime.world.footprint_mask().to_vec(),
            visible_tiles,
            explored_tiles,
            obstacles,
            deployables,
            players,
            projectiles,
            training_metrics: None,
        })
    }

    pub(super) fn build_arena_delta_snapshot(
        &mut self,
        match_id: MatchId,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
    ) -> Option<ArenaDeltaSnapshot> {
        let runtime = self.matches.get_mut(&match_id)?;
        let (visible_tiles, explored_tiles) = Self::build_visibility_masks(
            &runtime.world,
            &mut runtime.explored_tiles,
            viewer_id,
            map,
        )?;
        let obstacles =
            Self::arena_obstacles_snapshot(runtime.world.obstacles(), map, &explored_tiles);
        let deployables =
            Self::arena_deployables_snapshot(&runtime.world, viewer_id, map, &visible_tiles);
        let players = Self::arena_players_snapshot(runtime, viewer_id, map, &visible_tiles);
        let projectiles =
            Self::arena_projectiles_snapshot(&runtime.world, viewer_id, map, &visible_tiles);
        let (phase, phase_seconds_remaining) = Self::arena_match_phase_snapshot(&runtime.session);

        Some(ArenaDeltaSnapshot {
            mode: ArenaSessionMode::Match,
            phase,
            phase_seconds_remaining,
            tile_units: map.tile_units,
            footprint_tiles: runtime.world.footprint_mask().to_vec(),
            visible_tiles,
            explored_tiles,
            obstacles,
            deployables,
            players,
            projectiles,
            training_metrics: None,
        })
    }

    pub(super) fn build_training_state_snapshot(
        &mut self,
        training_id: MatchId,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
    ) -> Option<ArenaStateSnapshot> {
        let runtime = self.training_sessions.get_mut(&training_id)?;
        let (visible_tiles, explored_tiles) = Self::build_visibility_masks(
            &runtime.world,
            &mut runtime.explored_tiles,
            viewer_id,
            map,
        )?;
        let obstacles =
            Self::arena_obstacles_snapshot(runtime.world.obstacles(), map, &explored_tiles);
        let deployables =
            Self::arena_deployables_snapshot(&runtime.world, viewer_id, map, &visible_tiles);
        let players =
            Self::arena_training_players_snapshot(runtime, viewer_id, map, &visible_tiles);
        let projectiles =
            Self::arena_projectiles_snapshot(&runtime.world, viewer_id, map, &visible_tiles);

        Some(ArenaStateSnapshot {
            mode: ArenaSessionMode::Training,
            phase: game_net::ArenaMatchPhase::Combat,
            phase_seconds_remaining: None,
            width: runtime.world.arena_width_units(),
            height: runtime.world.arena_height_units(),
            tile_units: map.tile_units,
            footprint_tiles: runtime.world.footprint_mask().to_vec(),
            visible_tiles,
            explored_tiles,
            obstacles,
            deployables,
            players,
            projectiles,
            training_metrics: Some(Self::training_metrics_snapshot(runtime)),
        })
    }

    pub(super) fn build_training_delta_snapshot(
        &mut self,
        training_id: MatchId,
        viewer_id: PlayerId,
        map: &ArenaMapDefinition,
    ) -> Option<ArenaDeltaSnapshot> {
        let runtime = self.training_sessions.get_mut(&training_id)?;
        let (visible_tiles, explored_tiles) = Self::build_visibility_masks(
            &runtime.world,
            &mut runtime.explored_tiles,
            viewer_id,
            map,
        )?;
        let obstacles =
            Self::arena_obstacles_snapshot(runtime.world.obstacles(), map, &explored_tiles);
        let deployables =
            Self::arena_deployables_snapshot(&runtime.world, viewer_id, map, &visible_tiles);
        let players =
            Self::arena_training_players_snapshot(runtime, viewer_id, map, &visible_tiles);
        let projectiles =
            Self::arena_projectiles_snapshot(&runtime.world, viewer_id, map, &visible_tiles);

        Some(ArenaDeltaSnapshot {
            mode: ArenaSessionMode::Training,
            phase: game_net::ArenaMatchPhase::Combat,
            phase_seconds_remaining: None,
            tile_units: map.tile_units,
            footprint_tiles: runtime.world.footprint_mask().to_vec(),
            visible_tiles,
            explored_tiles,
            obstacles,
            deployables,
            players,
            projectiles,
            training_metrics: Some(Self::training_metrics_snapshot(runtime)),
        })
    }

    pub(super) fn broadcast_arena_state_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
    ) {
        let recipients = self.match_recipients(match_id);
        if recipients.is_empty() {
            return;
        }
        let map = self.content.map().clone();
        for recipient in recipients {
            let Some(snapshot) = self.build_arena_state_snapshot(match_id, recipient, &map) else {
                continue;
            };
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaStateSnapshot { snapshot },
            );
        }
    }

    pub(super) fn broadcast_training_state_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        training_id: MatchId,
    ) {
        let recipients = self.training_recipients(training_id);
        if recipients.is_empty() {
            return;
        }
        let Some(map) = self.content.training_map().cloned() else {
            return;
        };
        for recipient in recipients {
            let Some(snapshot) = self.build_training_state_snapshot(training_id, recipient, &map)
            else {
                continue;
            };
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaStateSnapshot { snapshot },
            );
        }
    }

    pub(super) fn broadcast_arena_delta_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
    ) {
        let recipients = self.match_recipients(match_id);
        if recipients.is_empty() {
            return;
        }
        let map = self.content.map().clone();
        for recipient in recipients {
            let Some(snapshot) = self.build_arena_delta_snapshot(match_id, recipient, &map) else {
                continue;
            };
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaDeltaSnapshot { snapshot },
            );
        }
    }

    pub(super) fn broadcast_training_delta_snapshot<T: AppTransport>(
        &mut self,
        transport: &mut T,
        training_id: MatchId,
    ) {
        let recipients = self.training_recipients(training_id);
        if recipients.is_empty() {
            return;
        }
        let Some(map) = self.content.training_map().cloned() else {
            return;
        };
        for recipient in recipients {
            let Some(snapshot) = self.build_training_delta_snapshot(training_id, recipient, &map)
            else {
                continue;
            };
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaDeltaSnapshot { snapshot },
            );
        }
    }

    pub(super) fn broadcast_arena_effect_batch<T: AppTransport>(
        &mut self,
        transport: &mut T,
        match_id: MatchId,
        effects: &[ArenaEffectSnapshot],
    ) {
        if effects.is_empty() {
            return;
        }

        let recipients = self.match_recipients(match_id);
        if recipients.is_empty() {
            return;
        }
        let map = self.content.map().clone();
        for recipient in recipients {
            let filtered = self.filter_arena_effects(match_id, recipient, effects, &map);
            if filtered.is_empty() {
                continue;
            }
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaEffectBatch { effects: filtered },
            );
        }
    }

    pub(super) fn broadcast_training_effect_batch<T: AppTransport>(
        &mut self,
        transport: &mut T,
        training_id: MatchId,
        effects: &[ArenaEffectSnapshot],
    ) {
        if effects.is_empty() {
            return;
        }
        let recipients = self.training_recipients(training_id);
        if recipients.is_empty() {
            return;
        }
        let Some(map) = self.content.training_map().cloned() else {
            return;
        };
        for recipient in recipients {
            let Some(runtime) = self.training_sessions.get_mut(&training_id) else {
                continue;
            };
            let Some((visible_tiles, _)) = Self::build_visibility_masks(
                &runtime.world,
                &mut runtime.explored_tiles,
                recipient,
                &map,
            ) else {
                continue;
            };
            let filtered = effects
                .iter()
                .copied()
                .filter(|effect| {
                    effect.owner == recipient
                        || Self::mask_contains_point(&map, &visible_tiles, effect.x, effect.y)
                        || Self::mask_contains_point(
                            &map,
                            &visible_tiles,
                            effect.target_x,
                            effect.target_y,
                        )
                })
                .collect::<Vec<_>>();
            if filtered.is_empty() {
                continue;
            }
            self.send_event(
                transport,
                recipient,
                ServerControlEvent::ArenaEffectBatch { effects: filtered },
            );
        }
    }

    pub(super) fn lobby_snapshot_phase(phase: &LobbyPhase) -> LobbySnapshotPhase {
        match phase {
            LobbyPhase::Open => LobbySnapshotPhase::Open,
            LobbyPhase::LaunchCountdown {
                seconds_remaining, ..
            } => LobbySnapshotPhase::LaunchCountdown {
                seconds_remaining: *seconds_remaining,
            },
        }
    }
}
