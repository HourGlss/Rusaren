# Lobby + Match + Round Flow

## Central lobby
- All connected players enter a shared central lobby.
- From the central lobby, players may create a game lobby or join an existing game lobby.
- The central lobby is not part of match simulation. It is a social and navigation space.

## Game lobby

### States
1) `GameLobbyOpen`
   - Players may join or leave the game lobby.
   - Players choose Team A or Team B.
   - There is no fixed team-size rule in v1. Players decide the team sizes.
   - Joining a game lobby sets that player's ready state to `NotReady`.
   - Players may change `NotReady -> Ready` and `Ready -> NotReady` while the launch countdown has not started.
   - Changing teams forces the player back to `NotReady`.
2) `LaunchCountdown`
   - Starts when every player currently in the game lobby is `Ready` and both teams have at least one player.
   - Countdown length is 5 seconds.
   - Once countdown starts, ready toggles are locked.
   - Once countdown starts, the match roster is locked.
   - The countdown cannot be canceled.
   - Players cannot voluntarily leave once the countdown has started.
   - If any countdown-locked player disconnects, the match is aborted immediately.
   - At countdown completion, the server allocates the match instance and moves those players into the game.

## Match
- A match always plays exactly 5 rounds.
- The team with the most round wins after round 5 wins the match.
- There is no early "first to 3 wins" match end in v1.
- After round 5, players are shown a win/lose and statistics screen.
- A player returns to the central lobby only after leaving that post-match screen by clicking `Quit`.
- Central lobby and game lobby both display each player's `W-L-NC` record.

## Round state machine
1) `RoundStart`
   - Reset player positions.
   - Clear round-scoped statuses and other round-only state.
   - Start the skill-pick phase.
2) `SkillPick`
   - Each player selects one skill node for that round.
   - Skill selection remains subject to the progression rules in `03_domain_model.md` and `11_classes.md`.
   - Skill-pick timer is 25 seconds.
3) `PreCombat`
   - Fixed 5-second countdown before live combat.
   - Loadouts are locked for the round.
4) `Combat`
   - Server simulation runs until a round win condition is met.
5) `RoundEnd`
   - Award the round win to one team.
   - If fewer than 5 rounds have been played, advance to the next round.
   - After round 5, transition to `MatchEnd`.
6) `MatchEnd`
   - Show the final win/lose result and match statistics.
   - Wait for the player to click `Quit` and return to the central lobby.

## Disconnect policy
- There is no reconnect-to-match flow in v1.
- If any player disconnects after the launch countdown has started, the current match is ended immediately.
- This applies during launch countdown, active combat, between rounds, and any other in-match phase.
- Voluntary leave is disabled once the launch countdown has started.
- Players are shown: `<PLAYER_NAME> has disconnected. Game is over.`
- A disconnect-ended match is recorded as `No Contest` for every player.

## Round win condition
- Primary win condition: a team wins the round when all opponents are dead or downed.
- Rounds do not timeout in v1.

## Remaining match-flow decisions
- None in the disconnect-abort flow currently documented here.
