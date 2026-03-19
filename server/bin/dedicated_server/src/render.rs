use game_lobby::LobbyEvent;
use game_match::MatchEvent;
use game_sim::SimulationEvent;

pub(crate) fn render_lobby_event(event: &LobbyEvent) -> String {
    match event {
        LobbyEvent::PlayerJoined { player_id } => {
            format!("player {} joined the lobby", player_id.get())
        }
        LobbyEvent::PlayerLeft { player_id } => {
            format!("player {} left the lobby", player_id.get())
        }
        LobbyEvent::TeamSelected {
            player_id,
            team,
            ready_reset,
        } => format!(
            "player {} joined {} (ready reset: {})",
            player_id.get(),
            team,
            ready_reset
        ),
        LobbyEvent::ReadyChanged { player_id, ready } => {
            format!("player {} ready state is now {:?}", player_id.get(), ready)
        }
        LobbyEvent::LaunchCountdownStarted {
            seconds_remaining,
            roster,
        } => format!(
            "launch countdown started at {} seconds for {} players",
            seconds_remaining,
            roster.len()
        ),
        LobbyEvent::LaunchCountdownTick { seconds_remaining } => {
            format!("launch countdown tick: {seconds_remaining}s remaining")
        }
        LobbyEvent::MatchLaunchReady { roster } => {
            format!("match launch ready with roster size {}", roster.len())
        }
        LobbyEvent::MatchAborted { message, .. } => format!("match aborted: {message}"),
    }
}

pub(crate) fn render_match_event(event: &MatchEvent) -> String {
    match event {
        MatchEvent::SkillChosen { player_id, choice } => format!(
            "player {} locked {:?} tier {}",
            player_id.get(),
            choice.tree,
            choice.tier
        ),
        MatchEvent::PreCombatStarted { seconds_remaining } => {
            format!("pre-combat started with {seconds_remaining}s remaining")
        }
        MatchEvent::CombatStarted => String::from("combat started"),
        MatchEvent::RoundWon {
            round,
            winning_team,
            score,
        } => format!(
            "round {} won by {} (score {}-{})",
            round.get(),
            winning_team,
            score.team_a,
            score.team_b
        ),
        MatchEvent::MatchEnded {
            outcome,
            message,
            score,
        } => format!(
            "match ended as {:?}: {} (score {}-{})",
            outcome, message, score.team_a, score.team_b
        ),
        MatchEvent::ManualResolutionRequired { reason } => {
            format!("manual resolution required: {reason}")
        }
    }
}

pub(crate) fn render_sim_event(event: &SimulationEvent) -> String {
    match event {
        SimulationEvent::PlayerMoved { player_id, x, y } => {
            format!("player {} moved to ({x}, {y})", player_id.get())
        }
        SimulationEvent::EffectSpawned { effect } => format!(
            "player {} spawned {:?} from ({}, {}) to ({}, {}) with radius {}",
            effect.owner.get(),
            effect.kind,
            effect.x,
            effect.y,
            effect.target_x,
            effect.target_y,
            effect.radius
        ),
        SimulationEvent::DamageApplied {
            attacker,
            target,
            amount,
            remaining_hit_points,
            defeated,
        } => format!(
            "player {} hit player {} for {} (remaining hp {}, defeated: {})",
            attacker.get(),
            target.get(),
            amount,
            remaining_hit_points,
            defeated
        ),
        SimulationEvent::HealingApplied {
            source,
            target,
            amount,
            resulting_hit_points,
        } => format!(
            "player {} healed player {} for {} (hp now {})",
            source.get(),
            target.get(),
            amount,
            resulting_hit_points
        ),
        SimulationEvent::StatusApplied {
            source,
            target,
            slot,
            kind,
            stacks,
            remaining_ms,
        } => format!(
            "player {} applied {:?} from slot {} to player {} (stacks {}, remaining {}ms)",
            source.get(),
            kind,
            slot,
            target.get(),
            stacks,
            remaining_ms
        ),
    }
}
