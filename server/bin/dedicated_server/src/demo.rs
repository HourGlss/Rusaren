use game_content::GameContent;
use game_domain::{
    LobbyId, MatchId, PlayerId, PlayerName, PlayerRecord, ReadyState, SkillChoice, SkillTree,
    TeamSide,
};
use game_lobby::{Lobby, LobbyEvent};
use game_match::{MatchConfig, MatchSession};
use game_sim::{MovementIntent, SimPlayerSeed, SimulationWorld};

use crate::render::{render_lobby_event, render_match_event, render_sim_event};

fn player_id(raw: u32) -> PlayerId {
    match PlayerId::new(raw) {
        Ok(player_id) => player_id,
        Err(error) => panic!("demo player id should be valid: {error}"),
    }
}

fn player_name(raw: &str) -> PlayerName {
    match PlayerName::new(raw) {
        Ok(player_name) => player_name,
        Err(error) => panic!("demo player name should be valid: {error}"),
    }
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    match SkillChoice::new(tree, tier) {
        Ok(choice) => choice,
        Err(error) => panic!("demo skill choice should be valid: {error}"),
    }
}

pub(crate) fn run_demo() -> Result<Vec<String>, String> {
    let alice_id = player_id(1);
    let bob_id = player_id(2);
    let content = GameContent::bundled().map_err(|error| error.to_string())?;
    let simulation = content.configuration().simulation;
    let mut lines = Vec::new();

    let alice_choice = skill(SkillTree::Mage, 1);
    let bob_choice = skill(SkillTree::Rogue, 1);
    let roster = build_demo_roster(&content, alice_id, bob_id, &mut lines)?;
    let mut session = build_demo_session(
        &content,
        roster.clone(),
        alice_id,
        bob_id,
        &alice_choice,
        &bob_choice,
        &mut lines,
    )?;
    let mut world = build_demo_world(&content, &roster, &alice_choice, &bob_choice, simulation)?;

    run_demo_combat(&mut world, alice_id, simulation.combat_frame_ms, &mut lines)?;

    for event in session
        .mark_player_defeated(bob_id)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_match_event(&event));
    }

    let disconnect_event = session
        .disconnect_player(alice_id)
        .map_err(|error| error.to_string())?;
    lines.push(render_match_event(&disconnect_event));

    Ok(lines)
}

fn build_demo_roster(
    content: &GameContent,
    alice_id: PlayerId,
    bob_id: PlayerId,
    lines: &mut Vec<String>,
) -> Result<Vec<game_domain::TeamAssignment>, String> {
    let mut lobby = Lobby::new(
        LobbyId::new(1).map_err(|error| error.to_string())?,
        content.configuration().lobby.launch_countdown_seconds,
    );
    lines.push(render_lobby_event(
        &lobby
            .add_player(alice_id, player_name("Alice"), PlayerRecord::new())
            .map_err(|error| error.to_string())?,
    ));
    lines.push(render_lobby_event(
        &lobby
            .add_player(bob_id, player_name("Bob"), PlayerRecord::new())
            .map_err(|error| error.to_string())?,
    ));
    append_lobby_events(
        lines,
        lobby.select_team(alice_id, TeamSide::TeamA)
            .map_err(|error| error.to_string())?,
    );
    append_lobby_events(
        lines,
        lobby.select_team(bob_id, TeamSide::TeamB)
            .map_err(|error| error.to_string())?,
    );
    append_lobby_events(
        lines,
        lobby.set_ready(alice_id, ReadyState::Ready)
            .map_err(|error| error.to_string())?,
    );
    append_lobby_events(
        lines,
        lobby.set_ready(bob_id, ReadyState::Ready)
            .map_err(|error| error.to_string())?,
    );

    let mut roster = None;
    for _ in 0..content.configuration().lobby.launch_countdown_seconds {
        let event = lobby
            .advance_countdown()
            .map_err(|error| error.to_string())?;
        if let LobbyEvent::MatchLaunchReady {
            roster: launch_roster,
        } = &event
        {
            roster = Some(launch_roster.clone());
        }
        lines.push(render_lobby_event(&event));
    }

    roster.ok_or_else(|| String::from("demo never produced a launch roster"))
}

fn build_demo_session(
    content: &GameContent,
    roster: Vec<game_domain::TeamAssignment>,
    alice_id: PlayerId,
    bob_id: PlayerId,
    alice_choice: &SkillChoice,
    bob_choice: &SkillChoice,
    lines: &mut Vec<String>,
) -> Result<MatchSession, String> {
    let mut session = MatchSession::new(
        MatchId::new(1).map_err(|error| error.to_string())?,
        roster,
        MatchConfig::new(
            content.configuration().match_flow.total_rounds,
            content.configuration().match_flow.skill_pick_seconds,
            content.configuration().match_flow.pre_combat_seconds,
            content.map().objective_target_ms,
        )
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    append_match_events(
        lines,
        session
            .submit_skill_pick(alice_id, alice_choice.clone())
            .map_err(|error| error.to_string())?,
    );
    append_match_events(
        lines,
        session
            .submit_skill_pick(bob_id, bob_choice.clone())
            .map_err(|error| error.to_string())?,
    );
    append_match_events(
        lines,
        session
            .advance_phase_by(content.configuration().match_flow.pre_combat_seconds)
            .map_err(|error| error.to_string())?,
    );
    Ok(session)
}

fn build_demo_world(
    content: &GameContent,
    roster: &[game_domain::TeamAssignment],
    alice_choice: &SkillChoice,
    bob_choice: &SkillChoice,
    simulation: game_content::SimulationConfiguration,
) -> Result<SimulationWorld, String> {
    SimulationWorld::new(
        vec![
            build_demo_player_seed(content, roster[0].clone(), alice_choice)?,
            build_demo_player_seed(content, roster[1].clone(), bob_choice)?,
        ],
        content.map(),
        simulation,
    )
    .map_err(|error| error.to_string())
}

fn build_demo_player_seed(
    content: &GameContent,
    assignment: game_domain::TeamAssignment,
    choice: &SkillChoice,
) -> Result<SimPlayerSeed, String> {
    let profile = content
        .class_profile(&choice.tree)
        .ok_or_else(|| String::from("demo class profile should exist"))?;
    let melee = content
        .skills()
        .melee_for(&choice.tree)
        .ok_or_else(|| String::from("demo melee content should exist"))?
        .clone();
    Ok(SimPlayerSeed {
        assignment,
        hit_points: profile.hit_points,
        max_mana: profile.max_mana,
        move_speed_units_per_second: profile.move_speed_units_per_second,
        melee,
        skills: [content.skills().resolve(choice).cloned(), None, None, None, None],
    })
}

fn run_demo_combat(
    world: &mut SimulationWorld,
    alice_id: PlayerId,
    combat_frame_ms: u16,
    lines: &mut Vec<String>,
) -> Result<(), String> {
    world
        .submit_input(
            alice_id,
            MovementIntent::new(1, 0).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    append_sim_events(lines, world.tick(combat_frame_ms));

    world
        .queue_cast(alice_id, 1)
        .map_err(|error| error.to_string())?;
    for _ in 0..6 {
        append_sim_events(lines, world.tick(combat_frame_ms));
    }
    Ok(())
}

fn append_lobby_events(lines: &mut Vec<String>, events: Vec<LobbyEvent>) {
    for event in events {
        lines.push(render_lobby_event(&event));
    }
}

fn append_match_events(lines: &mut Vec<String>, events: Vec<game_match::MatchEvent>) {
    for event in events {
        lines.push(render_match_event(&event));
    }
}

fn append_sim_events(lines: &mut Vec<String>, events: Vec<game_sim::SimulationEvent>) {
    for event in events {
        lines.push(render_sim_event(&event));
    }
}
