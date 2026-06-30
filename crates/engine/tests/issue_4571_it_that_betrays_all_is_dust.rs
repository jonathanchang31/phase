//! Issue #4571 regression: a delayed "return that card" zone move must no-op
//! once the referenced object has ceased to exist instead of panicking.
//!
//! Repro shape: a sacrifice trigger ("Whenever an opponent sacrifices a
//! nontoken permanent, put that card onto the battlefield under your control.")
//! watches `All Is Dust`. If the sacrificed set contains a colored TOKEN
//! alongside a colored nontoken permanent, only the card may return. The token
//! ceases to exist in the graveyard before any later return effect can act on
//! it, so the targeted `ChangeZone` continuation must skip the missing object
//! rather than calling `move_to_zone` on a non-existent `ObjectId`.
//!
//! CR 111.7: a token in a zone other than the battlefield ceases to exist.
//! CR 400.7: a zone-change effect can move an object only if that object still
//! exists in the expected origin zone.

use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaCost, ManaCostShard, ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const SACRIFICE_REANIMATOR: &str =
    "Whenever an opponent sacrifices a nontoken permanent, put that card onto the battlefield under your control.";
const ALL_IS_DUST: &str = "Each player sacrifices all colored permanents they control.";

fn mana(color: ManaType) -> ManaUnit {
    ManaUnit::new(color, ObjectId(0), false, vec![])
}

fn pool(count: usize, color: ManaType) -> Vec<ManaUnit> {
    (0..count).map(|_| mana(color)).collect()
}

fn on_battlefield_under(
    runner: &GameRunner,
    object_id: ObjectId,
    controller: engine::types::player::PlayerId,
) -> bool {
    runner
        .state()
        .objects
        .get(&object_id)
        .is_some_and(|obj| obj.zone == Zone::Battlefield && obj.controller == controller)
}

#[test]
fn all_is_dust_with_token_sacrifice_does_not_crash_it_that_betrays_return() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    scenario.with_mana_pool(P0, pool(7, ManaType::Colorless));

    scenario
        .add_creature(P0, "It That Betrays", 11, 11)
        .from_oracle_text(SACRIFICE_REANIMATOR);

    let stolen_card = scenario
        .add_creature(P1, "Scarlet Bear", 2, 2)
        .with_mana_cost(ManaCost::Cost {
            shards: vec![ManaCostShard::Red],
            generic: 1,
        })
        .id();
    let doomed_token = scenario
        .add_creature(P1, "Goblin Token", 1, 1)
        .with_mana_cost(ManaCost::Cost {
            shards: vec![ManaCostShard::Red],
            generic: 0,
        })
        .id();

    let all_is_dust = scenario
        .add_spell_to_hand_from_oracle(P0, "All Is Dust", false, ALL_IS_DUST)
        .with_mana_cost(ManaCost::Cost {
            shards: vec![],
            generic: 7,
        })
        .id();

    let mut runner = scenario.build();
    runner
        .state_mut()
        .objects
        .get_mut(&doomed_token)
        .expect("token exists")
        .is_token = true;

    let _outcome = runner.cast(all_is_dust).resolve();

    assert!(
        on_battlefield_under(&runner, stolen_card, P0),
        "the sacrificed nontoken permanent must return under P0's control"
    );
    assert!(
        !on_battlefield_under(&runner, doomed_token, P0)
            && !on_battlefield_under(&runner, doomed_token, P1),
        "the sacrificed token must not return to the battlefield"
    );
}
