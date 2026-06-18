//! DIAGNOSTIC for issue #3681 (Inferno Titan divided damage trigger).

use engine::game::scenario::{GameScenario, P0, P1};
use engine::types::actions::GameAction;
use engine::types::game_state::{CastPaymentMode, WaitingFor};
use engine::types::mana::{ManaType, ManaUnit};
use engine::types::phase::Phase;

const INFERNO_ORACLE: &str =
    "Whenever this creature enters or attacks, it deals 3 damage divided as you choose among one, two, or three targets.";

#[test]
fn zz_inferno_etb_trigger_target_flow() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    // Two legal creature targets for the opponent + the opponent player.
    let _bear = scenario.add_creature(P1, "Bear", 2, 2).id();
    let _elf = scenario.add_creature(P1, "Elf", 1, 1).id();

    let titan = scenario
        .add_creature_to_hand_from_oracle(P0, "Inferno Titan", 6, 6, INFERNO_ORACLE)
        .with_mana_cost(engine::types::mana::ManaCost::Cost {
            generic: 4,
            shards: vec![engine::types::mana::ManaCostShard::Red, engine::types::mana::ManaCostShard::Red],
        })
        .id();

    scenario.with_mana_pool(
        P0,
        vec![ManaUnit::new(
            ManaType::Red,
            engine::types::identifiers::ObjectId(0),
            false,
            vec![],
        ); 8],
    );

    let mut runner = scenario.build();
    let card_id = runner.state().objects[&titan].card_id;
    runner
        .act(GameAction::CastSpell {
            object_id: titan,
            card_id,
            targets: vec![],
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("begin casting Inferno Titan");

    // Drain mana-payment / cast flow until the ETB trigger target selection surfaces.
    for step in 0..60 {
        let wf = runner.state().waiting_for.clone();
        println!("ZZ_FLOW step={step} waiting_for={}", wf.variant_name());
        match wf {
            WaitingFor::Priority { .. } if !runner.state().stack.is_empty() => {
                runner.pass_both_players();
            }
            WaitingFor::Priority { .. } => break,
            WaitingFor::TriggerTargetSelection { target_slots, .. } => {
                println!("ZZ_FLOW TriggerTargetSelection slots={}", target_slots.len());
                for (i, s) in target_slots.iter().enumerate() {
                    println!("ZZ_FLOW   slot[{i}] optional={} n_legal={}", s.optional, s.legal_targets.len());
                }
                break;
            }
            WaitingFor::DistributeAmong { total, targets, .. } => {
                println!("ZZ_FLOW DistributeAmong total={total} n_targets={}", targets.len());
                break;
            }
            _ => runner.pass_both_players(),
        }
    }
    panic!("ZZ_FLOW done");
}
