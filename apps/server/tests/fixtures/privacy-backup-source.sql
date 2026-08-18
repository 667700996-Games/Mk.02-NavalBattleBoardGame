BEGIN;

INSERT INTO player_accounts (
  id,
  handle,
  recovery_key_hash,
  created_at
) VALUES (
  '00000000-0000-4000-8000-000000000010',
  'RestoreFixture',
  repeat('b', 64),
  now()
);

INSERT INTO user_sessions (
  id,
  account_id,
  nickname,
  token_hash,
  created_at,
  last_seen_at
) VALUES (
  '00000000-0000-4000-8000-000000000001',
  '00000000-0000-4000-8000-000000000010',
  'RestoreFixture',
  repeat('a', 64),
  now(),
  now()
);

INSERT INTO progression_reward_ledger (
  id,
  account_id,
  source_kind,
  source_id,
  period_key,
  xp
) VALUES (
  '00000000-0000-4000-8000-000000000011',
  '00000000-0000-4000-8000-000000000010',
  'MISSION',
  'RESTORE_DAILY',
  '2026-08-18',
  100
);

INSERT INTO ranked_ratings (
  account_id,
  rating,
  matches_played,
  season_id
) VALUES (
  '00000000-0000-4000-8000-000000000010',
  1530,
  5,
  'RESTORE_SEASON'
);

INSERT INTO ranked_season_standings (
  account_id,
  season_id,
  rating,
  matches_played,
  wins,
  losses,
  peak_rating
) VALUES (
  '00000000-0000-4000-8000-000000000010',
  'RESTORE_SEASON',
  1530,
  5,
  3,
  2,
  1540
);

INSERT INTO ranked_reward_ledger (
  id,
  account_id,
  source_kind,
  source_id,
  season_id,
  xp
) VALUES (
  '00000000-0000-4000-8000-000000000012',
  '00000000-0000-4000-8000-000000000010',
  'RANKED_SEASON',
  'RESTORE_REWARD',
  'RESTORE_SEASON',
  250
);

INSERT INTO ranked_leaderboard_snapshots (
  id,
  season_id,
  generated_at,
  expires_at,
  archived
) VALUES (
  '00000000-0000-4000-8000-000000000050',
  'RESTORE_SEASON',
  now(),
  now() + interval '5 minutes',
  false
);

INSERT INTO ranked_leaderboard_snapshot_entries (
  snapshot_id,
  rank,
  account_id,
  rating,
  matches_played,
  wins,
  losses,
  peak_rating
) VALUES (
  '00000000-0000-4000-8000-000000000050',
  1,
  '00000000-0000-4000-8000-000000000010',
  1530,
  5,
  3,
  2,
  1540
);

INSERT INTO player_relationships (
  actor_identity_id,
  target_identity_id,
  target_nickname,
  muted,
  blocked
) VALUES (
  '00000000-0000-4000-8000-000000000010',
  '00000000-0000-4000-8000-000000000030',
  'Restore Peer',
  true,
  false
);

INSERT INTO player_reports (
  id,
  reporter_identity_id,
  target_identity_id,
  target_nickname,
  category,
  details,
  evidence
) VALUES
  (
    '00000000-0000-4000-8000-000000000040',
    '00000000-0000-4000-8000-000000000010',
    '00000000-0000-4000-8000-000000000030',
    'Restore Report Target',
    'OTHER',
    'privacy restore fixture',
    '{}'::jsonb
  ),
  (
    '00000000-0000-4000-8000-000000000041',
    '00000000-0000-4000-8000-000000000031',
    '00000000-0000-4000-8000-000000000032',
    'Unrelated Report Target',
    'OTHER',
    'direct moderation target fixture',
    '{}'::jsonb
  );

INSERT INTO player_moderation_actions (
  id,
  report_id,
  target_identity_id,
  operator_id,
  action_type,
  reason
) VALUES (
  '00000000-0000-4000-8000-000000000042',
  '00000000-0000-4000-8000-000000000041',
  '00000000-0000-4000-8000-000000000010',
  'restore-drill',
  'WARN',
  'direct account action fixture'
);

INSERT INTO integrity_signals (
  id,
  subject_identity_id,
  kind,
  severity,
  confidence,
  evidence,
  first_observed_at,
  last_observed_at
) VALUES (
  '00000000-0000-4000-8000-000000000060',
  '00000000-0000-4000-8000-000000000010',
  'AUTOMATION',
  3,
  0.9,
  '{}'::jsonb,
  now(),
  now()
);

INSERT INTO matchmaking_queue (session_id)
VALUES ('00000000-0000-4000-8000-000000000001');

COMMIT;
