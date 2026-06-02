use rand::Rng;

use crate::game::{quantity::resolve_quantity, triggers, zones};
use crate::types::ability::{Effect, EffectError, EffectKind, ResolvedAbility};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::ObjectId;
use crate::types::player::PlayerId;
use crate::types::zones::Zone;

/// CR 701.51b-c: To open an Attraction, put the top card of your Attraction
/// deck onto the battlefield. If there are no cards in that deck, nothing
/// happens for that open instruction.
pub fn resolve_open(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let count = match &ability.effect {
        Effect::OpenAttraction { count } => {
            resolve_quantity(state, count, ability.controller, ability.source_id).max(0)
        }
        _ => return Err(EffectError::MissingParam("OpenAttraction".to_string())),
    };

    for _ in 0..count {
        open_one_attraction(state, ability.controller, events);
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::OpenAttraction,
        source_id: ability.source_id,
    });
    Ok(())
}

/// CR 701.52a + CR 702.159a: Roll a six-sided die; each Attraction the player
/// controls with the rolled number lit up is visited.
pub fn resolve_visit(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    if !matches!(ability.effect, Effect::RollToVisitAttractions) {
        return Err(EffectError::MissingParam(
            "RollToVisitAttractions".to_string(),
        ));
    }

    roll_to_visit_attractions(state, ability.controller, events);
    events.push(GameEvent::EffectResolved {
        kind: EffectKind::RollToVisitAttractions,
        source_id: ability.source_id,
    });
    Ok(())
}

/// CR 505.5 + CR 703.4g: As the active player's precombat main phase begins,
/// that player rolls to visit their Attractions as a turn-based action.
pub(crate) fn perform_precombat_main_visit(
    state: &mut GameState,
    events: &mut Vec<GameEvent>,
) -> bool {
    let player = state.active_player;
    if !controls_any_attraction(state, player) {
        return false;
    }

    let event_start = events.len();
    roll_to_visit_attractions(state, player, events);
    let trigger_events: Vec<_> = events[event_start..].to_vec();
    triggers::process_triggers(state, &trigger_events);
    true
}

fn open_one_attraction(state: &mut GameState, player: PlayerId, events: &mut Vec<GameEvent>) {
    let Some(object_id) = state
        .attraction_decks
        .get_mut(&player)
        .and_then(|deck| deck.pop_front())
    else {
        return;
    };

    zones::move_to_zone(state, object_id, Zone::Battlefield, events);
    if state
        .objects
        .get(&object_id)
        .is_some_and(|obj| obj.zone == Zone::Battlefield)
    {
        events.push(GameEvent::AttractionOpened {
            player_id: player,
            object_id,
        });
    }
}

fn roll_to_visit_attractions(state: &mut GameState, player: PlayerId, events: &mut Vec<GameEvent>) {
    let result = state.rng.random_range(1..=6);
    events.push(GameEvent::DieRolled {
        player_id: player,
        sides: 6,
        result,
    });
    state.die_result_this_resolution = Some(result);

    for object_id in visitable_attractions(state, player, result) {
        events.push(GameEvent::AttractionVisited {
            player_id: player,
            object_id,
            result,
        });
    }
}

fn visitable_attractions(state: &GameState, player: PlayerId, result: u8) -> Vec<ObjectId> {
    state
        .battlefield
        .iter()
        .copied()
        .filter(|object_id| {
            state.objects.get(object_id).is_some_and(|obj| {
                obj.controller == player
                    && is_attraction(obj)
                    && obj.attraction_lights.contains(&result)
            })
        })
        .collect()
}

fn controls_any_attraction(state: &GameState, player: PlayerId) -> bool {
    state.battlefield.iter().copied().any(|object_id| {
        state.objects.get(&object_id).is_some_and(|obj| {
            obj.controller == player && is_attraction(obj) && !obj.attraction_lights.is_empty()
        })
    })
}

fn is_attraction(obj: &crate::game::game_object::GameObject) -> bool {
    obj.card_types
        .subtypes
        .iter()
        .any(|subtype| subtype == "Attraction")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::ability::{QuantityExpr, ResolvedAbility};
    use crate::types::card_type::CoreType;
    use crate::types::identifiers::{CardId, ObjectId};

    fn attraction_in_command_deck(state: &mut GameState, name: &str, lights: Vec<u8>) -> ObjectId {
        let id = create_object(
            state,
            CardId(state.next_object_id),
            PlayerId(0),
            name.to_string(),
            Zone::Command,
        );
        let obj = state.objects.get_mut(&id).unwrap();
        obj.card_types.core_types.push(CoreType::Artifact);
        obj.card_types.subtypes.push("Attraction".to_string());
        obj.attraction_lights = lights;
        state.command_zone.retain(|existing| *existing != id);
        state
            .attraction_decks
            .entry(PlayerId(0))
            .or_default()
            .push_back(id);
        id
    }

    #[test]
    fn open_attraction_moves_top_card_to_battlefield_and_emits_event() {
        let mut state = GameState::new_two_player(42);
        let attraction = attraction_in_command_deck(&mut state, "Balloon Stand", vec![2, 6]);
        let ability = ResolvedAbility::new(
            Effect::OpenAttraction {
                count: QuantityExpr::Fixed { value: 1 },
            },
            vec![],
            ObjectId(99),
            PlayerId(0),
        );
        let mut events = Vec::new();

        resolve_open(&mut state, &ability, &mut events).unwrap();

        assert!(state.battlefield.contains(&attraction));
        assert_eq!(
            state
                .attraction_decks
                .get(&PlayerId(0))
                .map(|deck| deck.len()),
            Some(0)
        );
        assert!(events.iter().any(|event| {
            matches!(
                event,
                GameEvent::AttractionOpened {
                    player_id: PlayerId(0),
                    object_id
                } if *object_id == attraction
            )
        }));
    }

    #[test]
    fn roll_to_visit_emits_events_for_only_lit_controlled_attractions() {
        let mut state = GameState::new_two_player(42);
        let lit = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Storybook Ride".to_string(),
            Zone::Battlefield,
        );
        let unlit = create_object(
            &mut state,
            CardId(2),
            PlayerId(0),
            "Wrong Lights".to_string(),
            Zone::Battlefield,
        );
        let opponent = create_object(
            &mut state,
            CardId(3),
            PlayerId(1),
            "Opponent Ride".to_string(),
            Zone::Battlefield,
        );
        for id in [lit, unlit, opponent] {
            let obj = state.objects.get_mut(&id).unwrap();
            obj.card_types.core_types.push(CoreType::Artifact);
            obj.card_types.subtypes.push("Attraction".to_string());
            obj.attraction_lights = vec![1, 2, 3, 4, 5, 6];
        }
        state
            .objects
            .get_mut(&unlit)
            .unwrap()
            .attraction_lights
            .clear();

        let ability = ResolvedAbility::new(
            Effect::RollToVisitAttractions,
            vec![],
            ObjectId(99),
            PlayerId(0),
        );
        let mut events = Vec::new();

        resolve_visit(&mut state, &ability, &mut events).unwrap();

        assert!(events.iter().any(|event| matches!(
            event,
            GameEvent::DieRolled {
                player_id: PlayerId(0),
                sides: 6,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            GameEvent::AttractionVisited {
                player_id: PlayerId(0),
                object_id,
                ..
            } if *object_id == lit
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            GameEvent::AttractionVisited { object_id, .. } if *object_id == unlit
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            GameEvent::AttractionVisited { object_id, .. } if *object_id == opponent
        )));
    }
}
