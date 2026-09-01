INSERT INTO balance_rulesets (
  version,
  manifest_schema_version,
  checksum,
  label,
  manifest,
  released_at,
  change_note
) VALUES (
  2,
  1,
  'b73b72f6dfdba8020f21b86065aefd26c81645a8669932a38fcaa2abe976b8cd',
  'Tactical Fleet',
  '{"schemaVersion":1,"rulesetVersion":2,"label":"Tactical Fleet","boardSize":10,"fleet":[{"kind":"CARRIER","cells":5},{"kind":"BATTLESHIP","cells":4},{"kind":"CRUISER","cells":3},{"kind":"SUBMARINE","cells":3},{"kind":"DESTROYER","cells":2}],"classicShotsPerTurn":1,"rapidTurnDurationSeconds":30,"maximumTurnDurationSeconds":300,"consecutiveTimeoutForfeit":3,"salvoShotPolicy":"SURVIVING_SHIPS","turnAdvancePolicy":"AFTER_SHOT_ALLOWANCE","duplicateTargetPolicy":"REJECT","victoryCondition":"SINK_ALL_SHIPS","fleetRevealPolicy":"MATCH_COMPLETE","tacticalSkills":{"unlockTurn":3,"maxSkillsPerTurn":1,"skills":[{"kind":"RAPID_FIRE","grade":"C","usesPerMatch":3,"maxCells":2,"targetPattern":"TWO_TARGETS"},{"kind":"CROSS_FIRE","grade":"B","usesPerMatch":2,"maxCells":5,"targetPattern":"ORTHOGONAL_CROSS"},{"kind":"AREA_ANNIHILATION","grade":"A","usesPerMatch":1,"maxCells":9,"targetPattern":"THREE_BY_THREE"}]}}'::JSONB,
  '2026-09-01T00:00:00Z',
  'Add the immutable opt-in tactical skill rules used by custom rooms.'
);
