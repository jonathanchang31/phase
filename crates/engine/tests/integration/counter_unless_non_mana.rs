//! Integration tests for **counter-spell "unless" costs that are NOT mana**
//! (issue #3466).
//!
//! Before the fix, the counter-spell parser recognised only the mana form of
//! "unless [player] pays [cost]" (`unless its controller pays {N}`).  Costs
//! expressed as *pay N life*, *sacrifice a [filter]*, or *discard a card* were
//! silently dropped, so the ability resolved as an unconditional counter.
//!
//! This file tests both layers:
//!
//! 1. **Parser layer** — `parse_oracle_text` produces an `AbilityDefinition`
//!    with `unless_pay` set to the correct `AbilityCost` variant and
//!    `payer = ParentTargetController`.
//!
//! 2. **Runtime layer** — the `unless_pay` modifier reaches the pipeline and
//!    surfaces `WaitingFor::UnlessPayment` for the targeted spell's controller.
//!    Paying suppresses the counter (target resolves); declining lets it through
//!    (target is countered).
//!
//! CR ANCHORS (verified against docs/MagicCompRules.txt):
//!   * CR 118.12  — Unless-pay form for spells and abilities.
//!   * CR 118.12a — "[Do something] unless [a player does something else]."
//!   * CR 118.3   — A player can't pay a cost they can't afford.
//!   * CR 119.4   — "If a player pays life … the player loses that much life."
//!   * CR 601.2h  — Partial payments are not allowed.
//!   * CR 608.3a  — A resolving permanent spell enters the battlefield.
//!   * CR 701.6a  — Countered spells go to their owner's graveyard.
//!   * CR 701.9   — Discard.
//!   * CR 701.21  — Sacrifice.

use engine::game::scenario::{GameScenario, P0, P1};
use engine::game::zones::create_object;
use engine::parser::oracle::parse_oracle_text;
use engine::types::ability::{
    AbilityCost, DiscardSelfScope, Effect, QuantityExpr, SacrificeCost, TargetFilter,
    UnlessPayModifier,
};
use engine::types::actions::GameAction;
use engine::types::card_type::CoreType;
use engine::types::game_state::{CastingVariant, StackEntry, StackEntryKind, WaitingFor};
use engine::types::identifiers::{CardId, ObjectId};
use engine::types::mana::{ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::zones::Zone;

// ─── Oracle texts under test ─────────────────────────────────────────────────

const DASH_HOPES: &str = "Counter target spell unless its controller pays 5 life.";
const COUNTER_SACRIFICE: &str =
    "Counter target spell unless its controller sacrifices a creature.";
const COUNTER_DISCARD: &str = "Counter target spell unless its controller discards a card.";

// ─── Parser tests ─────────────────────────────────────────────────────────────

/// CR 118.12 + CR 119.4: "unless its controller pays 5 life" must parse to
/// `AbilityCost::PayLife { Fixed(5) }` with `payer = ParentTargetController`.
/// Pre-fix this was silently dropped, so `unless_pay` was `None`.
#[test]
fn dash_hopes_parses_pay_life_unless_pay() {
    let parsed = parse_oracle_text(DASH_HOPES, "Dash Hopes", &[], &["Instant".to_string()], &[]);
    let ability = parsed
        .abilities
        .first()
        .expect("Dash Hopes must parse a spell ability");

    // CR 701.6: the effect is a counter.
    assert!(
        matches!(ability.effect.as_ref(), Effect::Counter { .. }),
        "effect must be Counter, got {:?}",
        ability.effect
    );

    // CR 118.12a + CR 119.4: unless_pay must carry the PayLife cost.
    assert!(
        matches!(
            &ability.unless_pay,
            Some(UnlessPayModifier {
                cost: AbilityCost::PayLife {
                    amount: QuantityExpr::Fixed { value: 5 },
                },
                payer: TargetFilter::ParentTargetController,
            })
        ),
        "unless_pay must be PayLife(5) with ParentTargetController, got {:?}",
        ability.unless_pay
    );
}

/// CR 118.12 + CR 701.21: "unless its controller sacrifices a creature" must
/// parse to `AbilityCost::Sacrifice` (count 1, Creature filter, controller-
/// relative) with `payer = ParentTargetController`.
#[test]
fn counter_sacrifice_parses_sacrifice_unless_pay() {
    let parsed = parse_oracle_text(
        COUNTER_SACRIFICE,
        "Counter-Sacrifice",
        &[],
        &["Instant".to_string()],
        &[],
    );
    let ability = parsed
        .abilities
        .first()
        .expect("Counter-Sacrifice must parse a spell ability");

    assert!(
        matches!(ability.effect.as_ref(), Effect::Counter { .. }),
        "effect must be Counter, got {:?}",
        ability.effect
    );

    // CR 118.12a + CR 701.21: unless_pay must carry a Sacrifice cost.
    let modifier = ability
        .unless_pay
        .as_ref()
        .expect("unless_pay must be Some for a sacrifice counter");
    assert!(
        matches!(modifier.payer, TargetFilter::ParentTargetController),
        "payer must be ParentTargetController, got {:?}",
        modifier.payer
    );
    assert!(
        matches!(&modifier.cost, AbilityCost::Sacrifice(SacrificeCost { .. })),
        "cost must be Sacrifice, got {:?}",
        modifier.cost
    );
}

/// CR 118.12 + CR 701.9: "unless its controller discards a card" must parse to
/// `AbilityCost::Discard { count: 1 }` with `payer = ParentTargetController`.
#[test]
fn counter_discard_parses_discard_unless_pay() {
    let parsed = parse_oracle_text(
        COUNTER_DISCARD,
        "Counter-Discard",
        &[],
        &["Instant".to_string()],
        &[],
    );
    let ability = parsed
        .abilities
        .first()
        .expect("Counter-Discard must parse a spell ability");

    assert!(
        matches!(ability.effect.as_ref(), Effect::Counter { .. }),
        "effect must be Counter, got {:?}",
        ability.effect
    );

    // CR 118.12a + CR 701.9: unless_pay must carry a Discard cost.
    let modifier = ability
        .unless_pay
        .as_ref()
        .expect("unless_pay must be Some for a discard counter");
    assert!(
        matches!(modifier.payer, TargetFilter::ParentTargetController),
        "payer must be ParentTargetController, got {:?}",
        modifier.payer
    );
    assert!(
        matches!(
            &modifier.cost,
            AbilityCost::Discard {
                count: QuantityExpr::Fixed { value: 1 },
                filter: None,
                self_scope: DiscardSelfScope::FromHand,
                ..
            }
        ),
        "cost must be Discard(1 card from hand), got {:?}",
        modifier.cost
    );
}

// ─── Runtime helpers ──────────────────────────────────────────────────────────

/// Inject a vanilla creature spell directly onto the stack for `controller`.
///
/// A creature (permanent) spell is used because its final zone is
/// unambiguous: countered → graveyard (CR 701.6a); resolved → battlefield
/// (CR 608.3a). An instant/sorcery would end in the graveyard in both cases
/// (CR 608.2m), making the counter signal indistinguishable.
fn put_creature_spell_on_stack(
    runner: &mut engine::game::scenario::GameRunner,
    controller: PlayerId,
    name: &str,
    card_id: CardId,
) -> ObjectId {
    let id = create_object(
        runner.state_mut(),
        card_id,
        controller,
        name.to_string(),
        Zone::Stack,
    );
    {
        let obj = runner.state_mut().objects.get_mut(&id).unwrap();
        obj.card_types.core_types = vec![CoreType::Creature];
        obj.base_card_types = obj.card_types.clone();
        obj.power = Some(2);
        obj.toughness = Some(2);
        obj.base_power = Some(2);
        obj.base_toughness = Some(2);
    }
    runner.state_mut().stack.push_back(StackEntry {
        id,
        source_id: id,
        controller,
        kind: StackEntryKind::Spell {
            card_id,
            ability: None,
            casting_variant: CastingVariant::Normal,
            actual_mana_spent: 0,
        },
    });
    id
}

/// Build a scenario where P0 holds a counter-unless instant and P1 has a
/// creature spell on the stack.  Returns `(runner, counter_spell_id, target_id)`.
fn setup_counter_unless(
    oracle_text: &str,
) -> (engine::game::scenario::GameRunner, ObjectId, ObjectId) {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let counter_builder =
        scenario.add_spell_to_hand_from_oracle(P0, "Counter-Unless", true, oracle_text);
    let counter_id = counter_builder.id();

    // Fund with one {U} mana unit so the cast is not rejected on an empty pool.
    scenario.with_mana_pool(
        P0,
        vec![ManaUnit::new(ManaType::Blue, counter_id, false, vec![])],
    );

    let mut runner = scenario.build();

    let target = put_creature_spell_on_stack(&mut runner, P1, "Opponent Creature", CardId(777));

    (runner, counter_id, target)
}

// ─── Runtime tests: pay-life form ─────────────────────────────────────────────

/// CR 118.12a + CR 119.4: When a "counter unless controller pays 5 life" spell
/// targets P1's spell, resolving must surface `WaitingFor::UnlessPayment` for
/// P1 with cost `PayLife(5)`.  Pre-fix the unless cost was silently dropped, so
/// the counter fired unconditionally without any prompt.
#[test]
fn counter_pay_life_surfaces_unless_payment_prompt() {
    let (mut runner, counter_id, target) = setup_counter_unless(DASH_HOPES);

    // drive_resolution stops at WaitingFor::UnlessPayment (not auto-answered).
    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    match &runner.state().waiting_for {
        WaitingFor::UnlessPayment { player, cost, .. } => {
            assert_eq!(*player, P1, "targeted spell's controller must be the payer");
            assert!(
                matches!(
                    cost,
                    AbilityCost::PayLife {
                        amount: QuantityExpr::Fixed { value: 5 }
                    }
                ),
                "unless-cost must be PayLife(5), got {cost:?}"
            );
        }
        other => panic!("expected WaitingFor::UnlessPayment, got {other:?}"),
    }
}

/// CR 118.12a + CR 701.6a: When P1 declines to pay the 5-life unless-cost,
/// the counter fires and P1's creature spell goes to the graveyard.
/// P1's life total is unchanged (no partial payment per CR 601.2h).
#[test]
fn counter_pay_life_declined_counters_the_spell() {
    let (mut runner, counter_id, target) = setup_counter_unless(DASH_HOPES);
    runner.state_mut().players[1].life = 20;

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    assert!(
        matches!(runner.state().waiting_for, WaitingFor::UnlessPayment { .. }),
        "must be at UnlessPayment prompt before declining"
    );
    runner
        .act(GameAction::PayUnlessCost { pay: false })
        .expect("declining the unless-cost must be accepted");

    // CR 701.6a: countered → graveyard.
    assert_eq!(
        runner.state().objects[&target].zone,
        Zone::Graveyard,
        "P1's spell must be in the graveyard after being countered"
    );
    // CR 601.2h: no life deducted on a declined payment.
    assert_eq!(
        runner.state().players[1].life,
        20,
        "P1's life total must be unchanged after declining to pay"
    );
}

/// CR 118.12a + CR 119.4 + CR 608.3a: When P1 pays the 5-life unless-cost,
/// the counter is suppressed and P1's creature spell resolves to the battlefield.
/// P1 loses exactly 5 life (CR 119.4).
#[test]
fn counter_pay_life_paid_suppresses_the_counter() {
    let (mut runner, counter_id, target) = setup_counter_unless(DASH_HOPES);
    runner.state_mut().players[1].life = 20;

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    assert!(
        matches!(runner.state().waiting_for, WaitingFor::UnlessPayment { .. }),
        "must be at UnlessPayment prompt before paying"
    );
    runner
        .act(GameAction::PayUnlessCost { pay: true })
        .expect("paying the unless-cost must be accepted");

    // CR 119.4: paying 5 life deducts exactly 5.
    assert_eq!(
        runner.state().players[1].life,
        15,
        "P1 must lose exactly 5 life when paying the unless-cost"
    );

    // CR 608.3a: the counter is suppressed; P1's creature spell is still on
    // the stack and will resolve to the battlefield once priority is passed.
    runner.advance_until_stack_empty();

    assert_eq!(
        runner.state().objects[&target].zone,
        Zone::Battlefield,
        "P1's spell must reach the battlefield when the unless-cost is paid"
    );
}

/// CR 118.3 + CR 601.2h: When P1 cannot afford the unless-cost (3 life < 5
/// required), the cost is unpayable.  A `pay: true` attempt is accepted but
/// the authority returns `Failed`, so the counter fires — P1's spell is
/// countered and no life is deducted.
#[test]
fn counter_pay_life_unpayable_still_counters() {
    let (mut runner, counter_id, target) = setup_counter_unless(DASH_HOPES);
    // P1 has only 3 life — cannot pay 5.
    runner.state_mut().players[1].life = 3;

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    assert!(
        matches!(runner.state().waiting_for, WaitingFor::UnlessPayment { .. }),
        "must be at UnlessPayment prompt"
    );
    // Attempt to pay even though it's unaffordable.
    runner
        .act(GameAction::PayUnlessCost { pay: true })
        .expect("attempting to pay an unpayable life cost must be accepted");

    // CR 118.3: unpayable — the counter fires anyway.
    assert_eq!(
        runner.state().objects[&target].zone,
        Zone::Graveyard,
        "P1's spell must be countered when the life cost is unpayable"
    );
    // CR 601.2h: no life deducted on an unpayable cost.
    assert_eq!(
        runner.state().players[1].life,
        3,
        "P1's life must not change when the cost is unpayable"
    );
}

// ─── Runtime tests: sacrifice form ────────────────────────────────────────────

/// CR 118.12a + CR 701.21: A "counter unless sacrifice a creature" spell
/// surfaces `WaitingFor::WardSacrificeChoice` for P1 when P1 has a creature
/// to sacrifice.
///
/// The engine first issues `WaitingFor::UnlessPayment { cost: Sacrifice }` to
/// ask P1 whether they want to pay; submitting `pay: true` transitions to
/// `WardSacrificeChoice` for the creature selection.
#[test]
fn counter_sacrifice_with_creature_surfaces_sacrifice_choice() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let counter_builder =
        scenario.add_spell_to_hand_from_oracle(P0, "Counter-Unless", true, COUNTER_SACRIFICE);
    let counter_id = counter_builder.id();
    scenario.with_mana_pool(
        P0,
        vec![ManaUnit::new(ManaType::Blue, counter_id, false, vec![])],
    );
    // P1 has a creature to sacrifice.
    scenario.add_creature(P1, "Grizzly Bears", 2, 2);

    let mut runner = scenario.build();
    let target =
        put_creature_spell_on_stack(&mut runner, P1, "Opponent Creature", CardId(777));

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    // The engine first presents the unless-payment prompt for P1.
    assert!(
        matches!(
            runner.state().waiting_for,
            WaitingFor::UnlessPayment { player: P1, .. }
        ),
        "expected UnlessPayment for P1 first, got {:?}",
        runner.state().waiting_for
    );

    // P1 chooses to pay — triggers the sacrifice-selection sub-prompt.
    runner
        .act(GameAction::PayUnlessCost { pay: true })
        .expect("choosing to pay the sacrifice cost must be accepted");

    // CR 118.12a + CR 701.21: with an eligible creature, the engine must now
    // prompt P1 to choose which creature to sacrifice.
    assert!(
        matches!(
            runner.state().waiting_for,
            WaitingFor::WardSacrificeChoice { player: P1, .. }
        ),
        "expected WardSacrificeChoice for P1 after pay: true, got {:?}",
        runner.state().waiting_for
    );
}

/// CR 118.3 + CR 701.21: A "counter unless sacrifice a creature" spell
/// counters the target spell when P1 has no creatures to sacrifice (cost
/// is unpayable per CR 118.3).
#[test]
fn counter_sacrifice_no_creatures_counters_spell() {
    let (mut runner, counter_id, target) = setup_counter_unless(COUNTER_SACRIFICE);

    // Verify P1 has no creatures on the battlefield (the default scenario has none).
    assert!(
        !runner
            .state()
            .objects
            .values()
            .any(|obj| obj.controller == P1
                && obj.zone == Zone::Battlefield
                && obj.card_types.core_types.contains(&CoreType::Creature)),
        "P1 must have no creatures for this test"
    );

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    // The sacrifice cost is unpayable (no eligible permanents). The engine
    // issues WaitingFor::UnlessPayment; a pay:true attempt is treated as
    // unaffordable (CR 118.3) and the counter fires.
    match &runner.state().waiting_for {
        WaitingFor::UnlessPayment { player, .. } => {
            assert_eq!(*player, P1);
            runner
                .act(GameAction::PayUnlessCost { pay: true })
                .expect("pay attempt must be accepted even when unaffordable");
        }
        // Some implementations may short-circuit directly when the cost is
        // statically unpayable; allow a clean Priority state here too.
        WaitingFor::Priority { .. } => {}
        other => panic!("unexpected WaitingFor: {other:?}"),
    }

    // CR 701.6a: spell is countered when the sacrifice cost is unpayable.
    assert_eq!(
        runner.state().objects[&target].zone,
        Zone::Graveyard,
        "P1's spell must be countered when the sacrifice cost is unpayable"
    );
}

// ─── Runtime tests: discard form ──────────────────────────────────────────────

/// CR 118.3 + CR 701.9: A "counter unless discard a card" spell counters the
/// target spell when P1 has no cards in hand (cost is unpayable per CR 118.3).
#[test]
fn counter_discard_no_cards_counters_spell() {
    let (mut runner, counter_id, target) = setup_counter_unless(COUNTER_DISCARD);

    // Verify P1 has an empty hand (the default scenario starts with no cards).
    assert!(
        runner
            .state()
            .players
            .iter()
            .find(|p| p.id == P1)
            .map(|p| p.hand.is_empty())
            .unwrap_or(false),
        "P1 must have no cards in hand for this test"
    );

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    // Discard cost is unpayable (empty hand). Same pattern as sacrifice test.
    match &runner.state().waiting_for {
        WaitingFor::UnlessPayment { player, .. } => {
            assert_eq!(*player, P1);
            runner
                .act(GameAction::PayUnlessCost { pay: true })
                .expect("pay attempt must be accepted even when unaffordable");
        }
        WaitingFor::Priority { .. } => {}
        other => panic!("unexpected WaitingFor: {other:?}"),
    }

    // CR 701.6a: spell is countered when the discard cost is unpayable.
    assert_eq!(
        runner.state().objects[&target].zone,
        Zone::Graveyard,
        "P1's spell must be countered when the discard cost is unpayable"
    );
}

/// CR 118.12a + CR 701.9: A "counter unless discard a card" spell surfaces
/// `WaitingFor::WardDiscardChoice` for P1 when P1 has a card to discard.
#[test]
fn counter_discard_with_card_surfaces_discard_choice() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let counter_builder =
        scenario.add_spell_to_hand_from_oracle(P0, "Counter-Unless", true, COUNTER_DISCARD);
    let counter_id = counter_builder.id();
    scenario.with_mana_pool(
        P0,
        vec![ManaUnit::new(ManaType::Blue, counter_id, false, vec![])],
    );
    // Give P1 a card in hand so the discard cost is payable.
    scenario.add_card_to_hand(P1, "Mountain");

    let mut runner = scenario.build();
    let target =
        put_creature_spell_on_stack(&mut runner, P1, "Opponent Creature", CardId(777));

    runner
        .cast(counter_id)
        .target_objects(&[target])
        .resolve();

    // CR 118.12a + CR 701.9: with a card in hand, the engine must prompt
    // P1 to choose which card to discard.
    assert!(
        matches!(
            runner.state().waiting_for,
            WaitingFor::WardDiscardChoice { player: P1, .. }
        ),
        "expected WardDiscardChoice for P1, got {:?}",
        runner.state().waiting_for
    );
}
