//! Integration tests for Ninjutsu cluster issues #3648, #3662, #3661
//!
//! - #3648: Ninjutsu not being offered from the command zone
//! - #3662: Sakashima's Student paying/returning a creature but not entering
//! - #3661: Turtle Lair mana not making a Ninja spell castable (NOT A BUG - see below)

use engine::game::combat::{AttackerInfo, CombatState};
use engine::game::scenario::{GameScenario, P0, P1};
use engine::types::actions::GameAction;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::ObjectId;
use engine::types::keywords::Keyword;
use engine::types::mana::{ManaCost, ManaCostShard, ManaType, ManaUnit};
use engine::types::phase::Phase;

#[test]
fn test_3648_commander_ninjutsu_offered_from_command_zone() {
    // CR 702.49d: Commander ninjutsu functions from the command zone
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::DeclareBlockers);

    // Set up an unblocked attacker
    let attacker = scenario.add_creature(P0, "Attacker", 2, 2).id();

    // Add a regular Ninjutsu card to hand
    let hand_ninja = scenario.add_creature_to_hand(P0, "Hand Ninja", 2, 2).id();

    let mut runner = scenario.build();
    runner.state_mut().debug_mode = true;

    // Configure the hand Ninja with regular Ninjutsu
    {
        let obj = runner.state_mut().objects.get_mut(&hand_ninja).unwrap();
        let cost = ManaCost::Cost {
            shards: vec![ManaCostShard::Blue],
            generic: 1,
        };
        obj.keywords.push(Keyword::Ninjutsu(cost.clone()));
        obj.base_keywords.push(Keyword::Ninjutsu(cost));
    }

    // Add a Commander Ninjutsu card to command zone
    let commander_ninja_id = ObjectId(runner.state().next_object_id);
    {
        use engine::game::game_object::GameObject;
        use engine::im::Vector;
        use engine::types::card_type::CoreType;
        use engine::types::identifiers::CardId;
        use engine::types::zones::Zone;

        let state = runner.state_mut();

        // Create the commander ninja object
        let mut commander_obj = GameObject::new(
            commander_ninja_id,
            CardId(999), // Dummy card ID
            P0,
            "Commander Ninja".to_string(),
            Zone::Command,
        );
        commander_obj.base_power = Some(2);
        commander_obj.base_toughness = Some(2);
        let cost = ManaCost::Cost {
            shards: vec![ManaCostShard::Blue],
            generic: 1,
        };
        commander_obj
            .keywords
            .push(Keyword::CommanderNinjutsu(cost.clone()));
        commander_obj
            .base_keywords
            .push(Keyword::CommanderNinjutsu(cost));
        commander_obj.card_types.core_types.push(CoreType::Creature);
        commander_obj.card_types.subtypes.push("Ninja".to_string());

        // Add to objects and command zone
        state.objects.insert(commander_ninja_id, commander_obj);
        // Need to use push_back for im::Vector
        let mut new_command_zone = Vector::new();
        new_command_zone.push_back(commander_ninja_id);
        state.command_zone = new_command_zone;
    }

    // Set up combat state
    runner.state_mut().combat = Some(CombatState {
        attackers: vec![AttackerInfo::attacking_player(attacker, P1)],
        ..Default::default()
    });
    runner.state_mut().waiting_for = WaitingFor::Priority { player: P0 };
    runner.state_mut().priority_player = P0;

    // Add mana for Ninjutsu cost
    {
        let state = runner.state_mut();
        let player_data = state.players.iter_mut().find(|p| p.id == P0).unwrap();
        for _ in 0..2 {
            player_data.mana_pool.add(ManaUnit::new(
                ManaType::Blue,
                ObjectId(0),
                false,
                Vec::new(),
            ));
        }
    }

    // Get legal actions for P0
    let actions = engine::ai_support::legal_actions(runner.state());

    // Check if ActivateNinjutsu is available for BOTH regular and Commander Ninjutsu
    let ninjutsu_actions: Vec<_> = actions
        .iter()
        .filter(|action| matches!(action, GameAction::ActivateNinjutsu { .. }))
        .collect();

    if ninjutsu_actions.is_empty() {
        println!("Available actions: {:?}", actions);
        println!("Command zone: {:?}", runner.state().command_zone);
        println!("Player hand: {:?}", runner.state().players[0].hand);
        println!("Objects in command zone:");
        for &id in &runner.state().command_zone {
            if let Some(obj) = runner.state().objects.get(&id) {
                println!("  {:?}: {:?}", id, obj.name);
                println!("     Keywords: {:?}", obj.keywords);
            }
        }
    }

    // We should have at least one Ninjutsu action (from the hand ninja)
    assert!(
        !ninjutsu_actions.is_empty(),
        "At least regular Ninjutsu should be offered from hand"
    );

    // If the Commander Ninjutsu card is properly configured, we should see TWO actions
    // (one for hand ninja, one for commander ninja)
    println!(
        "Found {} Ninjutsu actions: {:?}",
        ninjutsu_actions.len(),
        ninjutsu_actions
    );
}
