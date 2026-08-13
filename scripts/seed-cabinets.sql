-- Seed 1000 virtual cabinets for pagination / stress testing.
-- Usage:
--   PGPASSWORD=admin123 psql -h localhost -U ipma -d ipma -f scripts/seed-cabinets.sql
--
-- Idempotent: the helper room + cabinets are created with fixed names so
-- re-running skips already-existing rows. Cabinet names are unique per room.
-- A marker prefix `TEST-CAB-` distinguishes seeded data from real data and
-- makes cleanup trivial:
--   DELETE FROM cabinets WHERE name LIKE 'TEST-CAB-%';
--   DELETE FROM rooms   WHERE name  = 'TEST-DATACENTER';

-- 1. A dedicated test data-center room (created if missing).
INSERT INTO rooms (id, name, room_type, description, created_at, updated_at)
SELECT '11111111-0000-0000-0000-000000000001',
       'TEST-DATACENTER', 'DATA_CENTER',
       'Auto-seeded room for pagination testing',
       NOW(), NOW()
WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE name = 'TEST-DATACENTER');

-- 2. 1000 cabinets inside the test room. Varied capacity (mostly 42U, some
--    22U / 47U) and descriptions so the list/grid looks realistic. Names are
--    zero-padded so they sort naturally.
INSERT INTO cabinets (id, name, room_id, capacity, description, created_at, updated_at)
SELECT uuid_generate_v4(),
       'TEST-CAB-' || lpad(g::text, 4, '0'),
       '11111111-0000-0000-0000-000000000001',
       CASE WHEN g % 7 = 0 THEN 22 WHEN g % 5 = 0 THEN 47 ELSE 42 END,
       'Auto-seeded cabinet #' || g || ' for pagination testing',
       NOW() - (g || ' minute')::interval,
       NOW()
FROM generate_series(1, 1000) AS g
WHERE NOT EXISTS (
    SELECT 1 FROM cabinets WHERE name = 'TEST-CAB-' || lpad(g::text, 4, '0')
);

-- 3. Report the result.
SELECT 'rooms' AS table_name, count(*) AS rows FROM rooms
UNION ALL
SELECT 'cabinets', count(*) FROM cabinets
UNION ALL
SELECT 'seeded_test_cabinets', count(*) FROM cabinets WHERE name LIKE 'TEST-CAB-%';
