CREATE TABLE balance_rulesets (
  version SMALLINT PRIMARY KEY CHECK (version > 0),
  manifest_schema_version SMALLINT NOT NULL CHECK (manifest_schema_version > 0),
  checksum CHAR(64) NOT NULL UNIQUE,
  label VARCHAR(64) NOT NULL,
  manifest JSONB NOT NULL,
  released_at TIMESTAMPTZ NOT NULL,
  change_note VARCHAR(256) NOT NULL,
  CONSTRAINT balance_ruleset_checksum_format
    CHECK (checksum ~ '^[0-9a-f]{64}$'),
  CONSTRAINT balance_ruleset_label_not_blank
    CHECK (length(trim(label)) > 0),
  CONSTRAINT balance_ruleset_change_note
    CHECK (length(trim(change_note)) >= 8),
  CONSTRAINT balance_ruleset_manifest_object
    CHECK (jsonb_typeof(manifest) = 'object'),
  CONSTRAINT balance_ruleset_manifest_identity CHECK (
    (manifest->>'schemaVersion')::INTEGER = manifest_schema_version
    AND (manifest->>'rulesetVersion')::INTEGER = version
    AND manifest->>'label' = label
  ),
  UNIQUE (version, checksum)
);

INSERT INTO balance_rulesets (
  version,
  manifest_schema_version,
  checksum,
  label,
  manifest,
  released_at,
  change_note
) VALUES (
  1,
  1,
  '6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76',
  'Founders Fleet',
  '{"schemaVersion":1,"rulesetVersion":1,"label":"Founders Fleet","boardSize":10,"fleet":[{"kind":"CARRIER","cells":5},{"kind":"BATTLESHIP","cells":4},{"kind":"CRUISER","cells":3},{"kind":"SUBMARINE","cells":3},{"kind":"DESTROYER","cells":2}],"classicShotsPerTurn":1,"rapidTurnDurationSeconds":30,"maximumTurnDurationSeconds":300,"consecutiveTimeoutForfeit":3,"salvoShotPolicy":"SURVIVING_SHIPS","turnAdvancePolicy":"AFTER_SHOT_ALLOWANCE","duplicateTargetPolicy":"REJECT","victoryCondition":"SINK_ALL_SHIPS","fleetRevealPolicy":"MATCH_COMPLETE"}'::JSONB,
  '2026-08-03T00:00:00Z',
  'Backfill the immutable rules used by every match before the balance catalog.'
);

CREATE FUNCTION reject_balance_ruleset_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'balance rulesets are append-only';
END;
$$;

CREATE TRIGGER balance_rulesets_immutable
BEFORE UPDATE OR DELETE ON balance_rulesets
FOR EACH ROW EXECUTE FUNCTION reject_balance_ruleset_mutation();

ALTER TABLE game_rooms
  ADD COLUMN ruleset_version SMALLINT NOT NULL DEFAULT 1,
  ADD COLUMN balance_checksum CHAR(64) NOT NULL
    DEFAULT '6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76',
  ADD CONSTRAINT game_rooms_ruleset_version_positive CHECK (ruleset_version > 0),
  ADD CONSTRAINT game_rooms_balance_checksum_format
    CHECK (balance_checksum ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT game_rooms_balance_ruleset_fk
    FOREIGN KEY (ruleset_version, balance_checksum)
    REFERENCES balance_rulesets(version, checksum) NOT VALID;

ALTER TABLE game_rooms
  VALIDATE CONSTRAINT game_rooms_balance_ruleset_fk;

ALTER TABLE game_results
  ADD COLUMN ruleset_version SMALLINT NOT NULL DEFAULT 1,
  ADD COLUMN balance_checksum CHAR(64) NOT NULL
    DEFAULT '6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76',
  ADD COLUMN balance_manifest JSONB NOT NULL DEFAULT
    '{"schemaVersion":1,"rulesetVersion":1,"label":"Founders Fleet","boardSize":10,"fleet":[{"kind":"CARRIER","cells":5},{"kind":"BATTLESHIP","cells":4},{"kind":"CRUISER","cells":3},{"kind":"SUBMARINE","cells":3},{"kind":"DESTROYER","cells":2}],"classicShotsPerTurn":1,"rapidTurnDurationSeconds":30,"maximumTurnDurationSeconds":300,"consecutiveTimeoutForfeit":3,"salvoShotPolicy":"SURVIVING_SHIPS","turnAdvancePolicy":"AFTER_SHOT_ALLOWANCE","duplicateTargetPolicy":"REJECT","victoryCondition":"SINK_ALL_SHIPS","fleetRevealPolicy":"MATCH_COMPLETE"}'::JSONB,
  ADD CONSTRAINT game_results_ruleset_version_positive CHECK (ruleset_version > 0),
  ADD CONSTRAINT game_results_balance_checksum_format
    CHECK (balance_checksum ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT game_results_balance_manifest_object
    CHECK (jsonb_typeof(balance_manifest) = 'object'),
  ADD CONSTRAINT game_results_balance_identity CHECK (
    (balance_manifest->>'rulesetVersion')::INTEGER = ruleset_version
  ),
  ADD CONSTRAINT game_results_balance_ruleset_fk
    FOREIGN KEY (ruleset_version, balance_checksum)
    REFERENCES balance_rulesets(version, checksum) NOT VALID;

ALTER TABLE game_results
  VALIDATE CONSTRAINT game_results_balance_ruleset_fk;

CREATE INDEX game_results_ruleset_finished_idx
  ON game_results (ruleset_version, finished_at DESC);
