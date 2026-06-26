//! Regression coverage for the "Choose a source [qualifier]" preamble
//! threading through the chunk loop into the `ChosenDamageSource` filter.
//!
//! Desperate Gambit's full Oracle text spans four chunks after
//! `split_clause_sequence`:
//!
//!   chunk 0: "Choose a source you control."
//!   chunk 1: "Flip a coin."
//!   chunk 2: "If you win the flip, the next time that source would deal
//!             damage this turn, it deals double that damage instead."
//!   chunk 3: "If you lose the flip, the next time it would deal damage
//!             this turn, prevent that damage."
//!
//! Chunk 0 sets `pending_source_qualifier = Some(controller(You))` via the
//! preamble detector; chunks 1–3 must each see that qualifier. Chunk 1
//! ("Flip a coin") does NOT match the one-shot replacement parser, so the
//! qualifier must SURVIVE that chunk rather than being eagerly consumed.
//! Chunk 2 produces a `CreateDamageReplacement` whose `source_filter` is
//! `ChosenDamageSource`; the qualifier must land on
//! `chosen_source_candidate_filter`.
//!
//! Bug fix evidence: without the qualifier threading, the win-branch effect
//! falls back to the hardcoded `TargetFilter::Any` enumeration — letting
//! the controller pick an opponent's source. This test fails closed on
//! that regression.

use engine::parser::oracle_effect::parse_effect_chain;
use engine::types::ability::{
    AbilityKind, ControllerRef, DamageModification, Effect, TargetFilter,
};

const DESPERATE_GAMBIT_ORACLE: &str = "Choose a source you control. Flip a coin. If you win the \
flip, the next time that source would deal damage this turn, it deals double that damage instead. \
If you lose the flip, the next time it would deal damage this turn, prevent that damage.";

#[test]
fn desperate_gambit_preamble_qualifier_threads_to_win_branch() {
    let def = parse_effect_chain(DESPERATE_GAMBIT_ORACLE, AbilityKind::Spell);

    let Effect::FlipCoin {
        win_effect: Some(win_effect),
        ..
    } = def.effect.as_ref()
    else {
        panic!(
            "Desperate Gambit must reduce to a FlipCoin with a win branch, got {:?}",
            def.effect
        );
    };

    let Effect::CreateDamageReplacement {
        source_filter: Some(TargetFilter::ChosenDamageSource),
        modification: Some(DamageModification::Double),
        chosen_source_candidate_filter: Some(candidate_filter),
        ..
    } = win_effect.effect.as_ref()
    else {
        panic!(
            "win branch must be a ChosenDamageSource-rooted CreateDamageReplacement \
             with a Double modification, got {:?}",
            win_effect.effect
        );
    };

    let TargetFilter::Typed(typed) = candidate_filter.as_ref() else {
        panic!(
            "candidate filter must be a Typed(You) filter, got {:?}",
            candidate_filter
        );
    };
    assert_eq!(
        typed.controller,
        Some(ControllerRef::You),
        "candidate filter must restrict candidates to controller-You"
    );
}
