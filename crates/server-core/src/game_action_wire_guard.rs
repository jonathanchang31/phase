//! Payload bounds for in-game `GameAction` bodies on the native WebSocket path.
//!
//! Engine legality checks still own game rules. This guard only rejects
//! transport-shape amplification before clone-heavy session handling.

use engine::types::ability::TargetRef;
use engine::types::actions::{
    DebugAction, DebugTokenRequest, GameAction, LearnOption, OutsideGameSelection,
};
use engine::types::game_state::{CounterMoveChoice, ManaChoice};
use engine::types::proposed_event::TokenCharacteristics;

/// Maximum items accepted in any action-level vector payload.
///
/// This is intentionally much larger than ordinary MTG choices but small
/// enough to reject megabyte-scale frames before the session lock path.
pub const MAX_GAME_ACTION_VEC_LEN: usize = 512;

/// Maximum UTF-8 scalar count for action-level client strings.
pub const MAX_GAME_ACTION_STRING_LEN: usize = 128;

fn guard_len(field: &str, len: usize, max: usize) -> Result<(), String> {
    if len > max {
        Err(format!("{field} must contain at most {max} items"))
    } else {
        Ok(())
    }
}

fn guard_vec<T>(field: &str, values: &[T]) -> Result<(), String> {
    guard_len(field, values.len(), MAX_GAME_ACTION_VEC_LEN)
}

fn guard_string(field: &str, value: &str) -> Result<(), String> {
    if value.chars().count() > MAX_GAME_ACTION_STRING_LEN {
        return Err(format!(
            "{field} must be at most {MAX_GAME_ACTION_STRING_LEN} characters"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}

fn guard_target_ref(field: &str, target: &TargetRef) -> Result<(), String> {
    match target {
        TargetRef::Object(_) | TargetRef::Player(_) => {
            let _ = field;
            Ok(())
        }
    }
}

fn guard_targets(field: &str, targets: &[TargetRef]) -> Result<(), String> {
    guard_vec(field, targets)?;
    for (idx, target) in targets.iter().enumerate() {
        guard_target_ref(&format!("{field}[{idx}]"), target)?;
    }
    Ok(())
}

fn guard_counter_moves(field: &str, selections: &[CounterMoveChoice]) -> Result<(), String> {
    guard_vec(field, selections)
}

fn guard_outside_game_selections(
    field: &str,
    selections: &[OutsideGameSelection],
) -> Result<(), String> {
    guard_vec(field, selections)?;
    for selection in selections {
        match selection {
            OutsideGameSelection::Sideboard { .. } | OutsideGameSelection::FaceUpExile { .. } => {}
        }
    }
    Ok(())
}

fn guard_mana_choice(field: &str, choice: &ManaChoice) -> Result<(), String> {
    match choice {
        ManaChoice::SingleColor(_) => Ok(()),
        ManaChoice::Combination(mana) => guard_vec(field, mana),
    }
}

fn guard_token_characteristics(
    field: &str,
    characteristics: &TokenCharacteristics,
) -> Result<(), String> {
    guard_string(
        &format!("{field}.display_name"),
        &characteristics.display_name,
    )?;
    guard_vec(&format!("{field}.core_types"), &characteristics.core_types)?;
    guard_vec(&format!("{field}.subtypes"), &characteristics.subtypes)?;
    for (idx, subtype) in characteristics.subtypes.iter().enumerate() {
        guard_string(&format!("{field}.subtypes[{idx}]"), subtype)?;
    }
    guard_vec(&format!("{field}.supertypes"), &characteristics.supertypes)?;
    guard_vec(&format!("{field}.colors"), &characteristics.colors)?;
    guard_vec(&format!("{field}.keywords"), &characteristics.keywords)
}

fn guard_debug_token_request(field: &str, request: &DebugTokenRequest) -> Result<(), String> {
    match request {
        DebugTokenRequest::Preset {
            preset_id,
            enter_with_counters,
            ..
        } => {
            guard_string(&format!("{field}.preset_id"), preset_id)?;
            guard_vec(&format!("{field}.enter_with_counters"), enter_with_counters)
        }
        DebugTokenRequest::Custom {
            characteristics,
            enter_with_counters,
            ..
        } => {
            guard_token_characteristics(&format!("{field}.characteristics"), characteristics)?;
            guard_vec(&format!("{field}.enter_with_counters"), enter_with_counters)
        }
    }
}

fn guard_debug_action(action: &DebugAction) -> Result<(), String> {
    match action {
        DebugAction::CreateCard { card_name, .. } => {
            guard_string("Debug.CreateCard.card_name", card_name)
        }
        DebugAction::AddMana { mana, .. } => guard_vec("Debug.AddMana.mana", mana),
        DebugAction::CreateToken { request } => {
            guard_debug_token_request("Debug.CreateToken.request", request)
        }
        DebugAction::MoveToZone { .. }
        | DebugAction::RemoveObject { .. }
        | DebugAction::DrawCards { .. }
        | DebugAction::Mill { .. }
        | DebugAction::ShuffleLibrary { .. }
        | DebugAction::Proliferate { .. }
        | DebugAction::SetBasePowerToughness { .. }
        | DebugAction::ModifyCounters { .. }
        | DebugAction::SetTapped { .. }
        | DebugAction::SetPrepared { .. }
        | DebugAction::SetController { .. }
        | DebugAction::SetSummoningSickness { .. }
        | DebugAction::SetFaceState { .. }
        | DebugAction::Attach { .. }
        | DebugAction::Detach { .. }
        | DebugAction::GrantKeyword { .. }
        | DebugAction::RemoveKeyword { .. }
        | DebugAction::SetLife { .. }
        | DebugAction::ModifyPlayerCounters { .. }
        | DebugAction::ModifyEnergy { .. }
        | DebugAction::SetPhase { .. }
        | DebugAction::RunStateBasedActions
        | DebugAction::CreateTokenCopy { .. } => Ok(()),
    }
}

/// Validate client-supplied `GameAction` payload fields before session dispatch.
pub fn guard_game_action_payload(action: &GameAction) -> Result<(), String> {
    match action {
        GameAction::CastSpell { targets, .. }
        | GameAction::CastSpellWithPaymentMode { targets, .. } => {
            guard_vec("CastSpell.targets", targets)
        }
        GameAction::DeclareAttackers { attacks } => guard_vec("DeclareAttackers.attacks", attacks),
        GameAction::DeclareBlockers { assignments } => {
            guard_vec("DeclareBlockers.assignments", assignments)
        }
        GameAction::ReorderHand { order } => guard_vec("ReorderHand.order", order),
        GameAction::SelectCards { cards } => guard_vec("SelectCards.cards", cards),
        GameAction::SelectCoinFlips { keep_indices } => {
            guard_vec("SelectCoinFlips.keep_indices", keep_indices)
        }
        GameAction::ChooseOutsideGameCards { selections } => {
            guard_outside_game_selections("ChooseOutsideGameCards.selections", selections)
        }
        GameAction::SelectTargets { targets } => guard_targets("SelectTargets.targets", targets),
        GameAction::ChooseTarget { target } => {
            if let Some(target) = target {
                guard_target_ref("ChooseTarget.target", target)?;
            }
            Ok(())
        }
        GameAction::OrderTriggers { order } => guard_vec("OrderTriggers.order", order),
        GameAction::CrewVehicle { creature_ids, .. } => {
            guard_vec("CrewVehicle.creature_ids", creature_ids)
        }
        GameAction::SaddleMount { creature_ids, .. } => {
            guard_vec("SaddleMount.creature_ids", creature_ids)
        }
        GameAction::SubmitSideboard { main, sideboard } => {
            guard_vec("SubmitSideboard.main", main)?;
            guard_vec("SubmitSideboard.sideboard", sideboard)
        }
        GameAction::ChooseOption { choice } => guard_string("ChooseOption.choice", choice),
        GameAction::SubmitPilePartition { pile_a } => {
            guard_vec("SubmitPilePartition.pile_a", pile_a)
        }
        GameAction::SelectModes { indices } => guard_vec("SelectModes.indices", indices),
        GameAction::SetPhaseStops { stops } => guard_vec("SetPhaseStops.stops", stops),
        GameAction::AssignCombatDamage { assignments, .. } => {
            guard_vec("AssignCombatDamage.assignments", assignments)
        }
        GameAction::DistributeAmong { distribution } => {
            guard_vec("DistributeAmong.distribution", distribution)?;
            for (idx, (target, _)) in distribution.iter().enumerate() {
                guard_target_ref(
                    &format!("DistributeAmong.distribution[{idx}].target"),
                    target,
                )?;
            }
            Ok(())
        }
        GameAction::ChooseCounterMoveDistribution { selections } => {
            guard_counter_moves("ChooseCounterMoveDistribution.selections", selections)
        }
        GameAction::RetargetSpell { new_targets } => {
            guard_targets("RetargetSpell.new_targets", new_targets)
        }
        GameAction::SelectCategoryPermanents { choices } => {
            guard_vec("SelectCategoryPermanents.choices", choices)
        }
        GameAction::SubmitPhyrexianChoices { choices } => {
            guard_vec("SubmitPhyrexianChoices.choices", choices)
        }
        GameAction::ChooseManaColor { choice, .. } => {
            guard_mana_choice("ChooseManaColor.choice", choice)
        }
        GameAction::PayManaAbilityMana { payment } => {
            guard_vec("PayManaAbilityMana.payment", payment)
        }
        GameAction::Debug(debug) => guard_debug_action(debug),
        GameAction::PassPriority
        | GameAction::PlayLand { .. }
        | GameAction::Foretell { .. }
        | GameAction::ActivateAbility { .. }
        | GameAction::ChooseUntap { .. }
        | GameAction::ChooseExert { .. }
        | GameAction::ChooseClashOpponent { .. }
        | GameAction::MulliganDecision { .. }
        | GameAction::TapLandForMana { .. }
        | GameAction::UntapLandForMana { .. }
        | GameAction::ChooseReplacement { .. }
        | GameAction::CancelCast
        | GameAction::Equip { .. }
        | GameAction::ActivateStation { .. }
        | GameAction::Transform { .. }
        | GameAction::PlayFaceDown { .. }
        | GameAction::TurnFaceUp { .. }
        | GameAction::ChoosePlayDraw { .. }
        | GameAction::ChoosePile { .. }
        | GameAction::ChooseBranch { .. }
        | GameAction::ChooseDamageSource { .. }
        | GameAction::DecideOptionalCost { .. }
        | GameAction::ChooseAdventureFace { .. }
        | GameAction::ChooseModalFace { .. }
        | GameAction::ChooseAlternativeCast { .. }
        | GameAction::ChooseCastingVariant { .. }
        | GameAction::KeepAllCopyTargets
        | GameAction::ChoosePermanentTypeSlot { .. }
        | GameAction::ActivateNinjutsu { .. }
        | GameAction::CastSpellAsSneak { .. }
        | GameAction::CastSpellAsSneakWithPaymentMode { .. }
        | GameAction::CastSpellAsWebSlinging { .. }
        | GameAction::CastSpellAsWebSlingingWithPaymentMode { .. }
        | GameAction::CastSpellForFree { .. }
        | GameAction::CastSpellForFreeWithPaymentMode { .. }
        | GameAction::CastSpellAsMiracle { .. }
        | GameAction::CastSpellAsMiracleWithPaymentMode { .. }
        | GameAction::CastSpellAsMadness { .. }
        | GameAction::CastSpellAsMadnessWithPaymentMode { .. }
        | GameAction::DecideOptionalEffect { .. }
        | GameAction::DecideOptionalEffectAndRemember { .. }
        | GameAction::PayUnlessCost { .. }
        | GameAction::ChooseUnlessCostBranch { .. }
        | GameAction::ChooseActivationCostBranch { .. }
        | GameAction::PayCombatTax { .. }
        | GameAction::ChooseRingBearer { .. }
        | GameAction::ChoosePair { .. }
        | GameAction::ChooseDungeon { .. }
        | GameAction::ChooseDungeonRoom { .. }
        | GameAction::UnlockRoomDoor { .. }
        | GameAction::TapForConvoke { .. }
        | GameAction::HarmonizeTap { .. }
        | GameAction::DeclareCompanion { .. }
        | GameAction::CompanionToHand
        | GameAction::DiscoverChoice { .. }
        | GameAction::CascadeChoice { .. }
        | GameAction::ChooseTopOrBottom { .. }
        | GameAction::ChooseLegend { .. }
        | GameAction::ChooseBattleProtector { .. }
        | GameAction::SetAutoPass { .. }
        | GameAction::CancelAutoPass
        | GameAction::SubmitPayAmount { .. }
        | GameAction::LearnDecision {
            choice: LearnOption::Skip,
        }
        | GameAction::LearnDecision {
            choice: LearnOption::Rummage { .. },
        }
        | GameAction::ChooseX { .. }
        | GameAction::CastPreparedCopy { .. }
        | GameAction::CastParadigmCopy { .. }
        | GameAction::PassParadigmOffer
        | GameAction::GrantDebugPermission { .. }
        | GameAction::RevokeDebugPermission { .. }
        | GameAction::Concede { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::types::card_type::CoreType;
    use engine::types::counter::CounterType;
    use engine::types::identifiers::ObjectId;
    use engine::types::mana::ManaColor;
    use engine::types::player::PlayerId;
    use engine::types::zones::Zone;

    #[test]
    fn accepts_small_action_payloads() {
        let action = GameAction::ReorderHand {
            order: vec![ObjectId(1), ObjectId(2), ObjectId(3)],
        };
        assert!(guard_game_action_payload(&action).is_ok());
    }

    #[test]
    fn rejects_issue_reproducer_large_reorder_hand() {
        let action = GameAction::ReorderHand {
            order: vec![ObjectId(0); 100_000],
        };
        let err = guard_game_action_payload(&action).unwrap_err();
        assert!(err.contains("ReorderHand.order"));
    }

    #[test]
    fn rejects_large_nested_target_vectors() {
        let action = GameAction::SelectTargets {
            targets: vec![TargetRef::Object(ObjectId(7)); MAX_GAME_ACTION_VEC_LEN + 1],
        };
        let err = guard_game_action_payload(&action).unwrap_err();
        assert!(err.contains("SelectTargets.targets"));
    }

    #[test]
    fn rejects_oversized_action_strings() {
        let action = GameAction::ChooseOption {
            choice: "x".repeat(MAX_GAME_ACTION_STRING_LEN + 1),
        };
        let err = guard_game_action_payload(&action).unwrap_err();
        assert!(err.contains("ChooseOption.choice"));
    }

    #[test]
    fn rejects_oversized_debug_card_name() {
        let action = GameAction::Debug(DebugAction::CreateCard {
            card_name: "x".repeat(MAX_GAME_ACTION_STRING_LEN + 1),
            owner: PlayerId(0),
            zone: Zone::Hand,
            attach_to: None,
        });
        let err = guard_game_action_payload(&action).unwrap_err();
        assert!(err.contains("Debug.CreateCard.card_name"));
    }

    #[test]
    fn rejects_nested_custom_token_subtype_flood() {
        let action = GameAction::Debug(DebugAction::CreateToken {
            request: DebugTokenRequest::Custom {
                owner: PlayerId(0),
                characteristics: TokenCharacteristics {
                    display_name: "Widget".to_string(),
                    power: Some(1),
                    toughness: Some(1),
                    core_types: vec![CoreType::Creature],
                    subtypes: vec!["Widget".to_string(); MAX_GAME_ACTION_VEC_LEN + 1],
                    supertypes: Vec::new(),
                    colors: vec![ManaColor::Green],
                    keywords: Vec::new(),
                },
                enter_with_counters: vec![(CounterType::Plus1Plus1, 1)],
            },
        });
        let err = guard_game_action_payload(&action).unwrap_err();
        assert!(err.contains("subtypes"));
    }
}
