use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};

use super::ShipKind;

pub const BALANCE_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const CURRENT_RULESET_VERSION: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalvoShotPolicy {
    SurvivingShips,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TurnAdvancePolicy {
    AfterShotAllowance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DuplicateTargetPolicy {
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VictoryCondition {
    SinkAllShips,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FleetRevealPolicy {
    MatchComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TacticalSkillKind {
    RapidFire,
    CrossFire,
    AreaAnnihilation,
}

impl TacticalSkillKind {
    pub const ALL: [Self; 3] = [Self::RapidFire, Self::CrossFire, Self::AreaAnnihilation];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TacticalSkillGrade {
    C,
    B,
    A,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TacticalSkillTargetPattern {
    TwoTargets,
    OrthogonalCross,
    ThreeByThree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TacticalSkillSpec {
    pub kind: TacticalSkillKind,
    pub grade: TacticalSkillGrade,
    pub uses_per_match: u8,
    pub max_cells: u8,
    pub target_pattern: TacticalSkillTargetPattern,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TacticalSkillRules {
    pub unlock_turn: u32,
    pub max_skills_per_turn: u8,
    pub skills: Vec<TacticalSkillSpec>,
}

impl TacticalSkillRules {
    pub fn standard() -> Self {
        Self {
            unlock_turn: 3,
            max_skills_per_turn: 1,
            skills: vec![
                TacticalSkillSpec {
                    kind: TacticalSkillKind::RapidFire,
                    grade: TacticalSkillGrade::C,
                    uses_per_match: 3,
                    max_cells: 2,
                    target_pattern: TacticalSkillTargetPattern::TwoTargets,
                },
                TacticalSkillSpec {
                    kind: TacticalSkillKind::CrossFire,
                    grade: TacticalSkillGrade::B,
                    uses_per_match: 2,
                    max_cells: 5,
                    target_pattern: TacticalSkillTargetPattern::OrthogonalCross,
                },
                TacticalSkillSpec {
                    kind: TacticalSkillKind::AreaAnnihilation,
                    grade: TacticalSkillGrade::A,
                    uses_per_match: 1,
                    max_cells: 9,
                    target_pattern: TacticalSkillTargetPattern::ThreeByThree,
                },
            ],
        }
    }

    pub fn spec(&self, kind: TacticalSkillKind) -> Option<TacticalSkillSpec> {
        self.skills.iter().find(|skill| skill.kind == kind).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalanceShipSpec {
    pub kind: ShipKind,
    pub cells: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalanceManifest {
    pub schema_version: u16,
    pub ruleset_version: u16,
    pub label: String,
    pub board_size: u8,
    pub fleet: Vec<BalanceShipSpec>,
    pub classic_shots_per_turn: u8,
    pub rapid_turn_duration_seconds: u32,
    pub maximum_turn_duration_seconds: u32,
    pub consecutive_timeout_forfeit: u8,
    pub salvo_shot_policy: SalvoShotPolicy,
    pub turn_advance_policy: TurnAdvancePolicy,
    pub duplicate_target_policy: DuplicateTargetPolicy,
    pub victory_condition: VictoryCondition,
    pub fleet_reveal_policy: FleetRevealPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tactical_skills: Option<TacticalSkillRules>,
}

impl BalanceManifest {
    pub fn v1() -> Self {
        Self {
            schema_version: BALANCE_MANIFEST_SCHEMA_VERSION,
            ruleset_version: 1,
            label: "Founders Fleet".to_string(),
            board_size: 10,
            fleet: vec![
                BalanceShipSpec {
                    kind: ShipKind::Carrier,
                    cells: 5,
                },
                BalanceShipSpec {
                    kind: ShipKind::Battleship,
                    cells: 4,
                },
                BalanceShipSpec {
                    kind: ShipKind::Cruiser,
                    cells: 3,
                },
                BalanceShipSpec {
                    kind: ShipKind::Submarine,
                    cells: 3,
                },
                BalanceShipSpec {
                    kind: ShipKind::Destroyer,
                    cells: 2,
                },
            ],
            classic_shots_per_turn: 1,
            rapid_turn_duration_seconds: 30,
            maximum_turn_duration_seconds: 300,
            consecutive_timeout_forfeit: 3,
            salvo_shot_policy: SalvoShotPolicy::SurvivingShips,
            turn_advance_policy: TurnAdvancePolicy::AfterShotAllowance,
            duplicate_target_policy: DuplicateTargetPolicy::Reject,
            victory_condition: VictoryCondition::SinkAllShips,
            fleet_reveal_policy: FleetRevealPolicy::MatchComplete,
            tactical_skills: None,
        }
    }

    pub fn v2() -> Self {
        let mut manifest = Self::v1();
        manifest.ruleset_version = 2;
        manifest.label = "Tactical Fleet".to_string();
        manifest.tactical_skills = Some(TacticalSkillRules::standard());
        manifest
    }

    pub fn registered(version: u16) -> Option<Self> {
        match version {
            1 => Some(Self::v1()),
            2 => Some(Self::v2()),
            _ => None,
        }
    }

    pub fn ship_cells(&self, kind: ShipKind) -> Option<u8> {
        self.fleet
            .iter()
            .find(|ship| ship.kind == kind)
            .map(|ship| ship.cells)
    }

    pub fn checksum(&self) -> String {
        let encoded = serde_json::to_vec(self).expect("balance manifest must serialize");
        format!("{:x}", Sha256::digest(encoded))
    }

    pub fn has_valid_shape(&self) -> bool {
        if self.schema_version != BALANCE_MANIFEST_SCHEMA_VERSION
            || self.ruleset_version == 0
            || !(5..=20).contains(&self.board_size)
            || self.label.trim().is_empty()
            || self.label.chars().count() > 64
            || self.classic_shots_per_turn == 0
            || self.classic_shots_per_turn > 10
            || self.rapid_turn_duration_seconds == 0
            || self.rapid_turn_duration_seconds > self.maximum_turn_duration_seconds
            || !(1..=600).contains(&self.maximum_turn_duration_seconds)
            || !(1..=10).contains(&self.consecutive_timeout_forfeit)
            || self.fleet.len() != ShipKind::ALL.len()
        {
            return false;
        }
        let mut kinds = std::collections::HashSet::new();
        let fleet_valid = self.fleet.iter().all(|ship| {
            kinds.insert(ship.kind)
                && ship.cells > 0
                && ship.cells <= self.board_size
                && ShipKind::ALL.contains(&ship.kind)
        }) && ShipKind::ALL.iter().all(|kind| kinds.contains(kind));
        if !fleet_valid {
            return false;
        }
        let Some(rules) = &self.tactical_skills else {
            return self.ruleset_version == 1;
        };
        if self.ruleset_version < 2
            || rules.unlock_turn < 3
            || rules.max_skills_per_turn != 1
            || rules.skills.len() != TacticalSkillKind::ALL.len()
        {
            return false;
        }
        let mut skill_kinds = std::collections::HashSet::new();
        rules.skills.iter().all(|skill| {
            skill_kinds.insert(skill.kind)
                && skill.uses_per_match > 0
                && skill.max_cells > 0
                && match skill.kind {
                    TacticalSkillKind::RapidFire => {
                        skill.grade == TacticalSkillGrade::C
                            && skill.uses_per_match == 3
                            && skill.max_cells == 2
                            && skill.target_pattern == TacticalSkillTargetPattern::TwoTargets
                    }
                    TacticalSkillKind::CrossFire => {
                        skill.grade == TacticalSkillGrade::B
                            && skill.uses_per_match == 2
                            && skill.max_cells == 5
                            && skill.target_pattern == TacticalSkillTargetPattern::OrthogonalCross
                    }
                    TacticalSkillKind::AreaAnnihilation => {
                        skill.grade == TacticalSkillGrade::A
                            && skill.uses_per_match == 1
                            && skill.max_cells == 9
                            && skill.target_pattern == TacticalSkillTargetPattern::ThreeByThree
                    }
                }
        }) && TacticalSkillKind::ALL
            .iter()
            .all(|kind| skill_kinds.contains(kind))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalancePin {
    pub ruleset_version: u16,
    pub checksum: String,
    pub manifest: BalanceManifest,
}

impl BalancePin {
    pub fn v1() -> Self {
        Self::from_manifest(BalanceManifest::v1())
    }

    pub fn v2() -> Self {
        Self::from_manifest(BalanceManifest::v2())
    }

    pub fn current() -> Self {
        Self::registered(CURRENT_RULESET_VERSION)
            .expect("the current balance ruleset must remain registered")
    }

    pub fn registered(version: u16) -> Option<Self> {
        BalanceManifest::registered(version).map(Self::from_manifest)
    }

    pub fn from_manifest(manifest: BalanceManifest) -> Self {
        Self {
            ruleset_version: manifest.ruleset_version,
            checksum: manifest.checksum(),
            manifest,
        }
    }

    pub fn has_valid_integrity(&self) -> bool {
        self.manifest.has_valid_shape()
            && self.ruleset_version == self.manifest.ruleset_version
            && self.checksum.len() == 64
            && self
                .checksum
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            && self.checksum == self.manifest.checksum()
    }

    pub fn is_registered_for_execution(&self) -> bool {
        self.has_valid_integrity() && Self::registered(self.ruleset_version).as_ref() == Some(self)
    }
}

impl Default for BalancePin {
    fn default() -> Self {
        // Missing pins can only originate from pre-catalog snapshots, all of which used V1.
        // This must never point at `current()`: doing so would reinterpret old matches after V2.
        Self::v1()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_manifest_has_a_stable_self_verifying_pin() {
        let pin = BalancePin::current();
        assert_eq!(pin.ruleset_version, CURRENT_RULESET_VERSION);
        assert!(pin.has_valid_integrity());
        assert!(pin.is_registered_for_execution());
        assert_eq!(pin.ruleset_version, 2);
        assert!(pin.manifest.tactical_skills.is_some());
        assert_eq!(pin.manifest.ship_cells(ShipKind::Carrier), Some(5));
        assert_eq!(pin.manifest.ship_cells(ShipKind::Destroyer), Some(2));
    }

    #[test]
    fn v1_manifest_checksum_remains_immutable() {
        let pin = BalancePin::v1();
        assert_eq!(
            pin.checksum,
            "6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76"
        );
        assert!(pin.manifest.tactical_skills.is_none());
        assert!(pin.is_registered_for_execution());
    }

    #[test]
    fn changed_or_unknown_rulesets_cannot_execute_silently() {
        let mut changed = BalanceManifest::v1();
        changed.consecutive_timeout_forfeit = 4;
        let self_contained = BalancePin::from_manifest(changed);
        assert!(self_contained.has_valid_integrity());
        assert!(!self_contained.is_registered_for_execution());

        let mut tampered = BalancePin::v1();
        tampered.manifest.rapid_turn_duration_seconds = 20;
        assert!(!tampered.has_valid_integrity());
        assert!(!tampered.is_registered_for_execution());
    }
}
