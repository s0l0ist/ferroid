# pg_ferroid

PostgreSQL extension for ULID (Universally Unique Lexicographically Sortable
Identifier) support, built with
[pgrx](https://github.com/pgcentralfoundation/pgrx) and
[ferroid](https://github.com/s0l0ist/ferroid/tree/main/crates/ferroid).

## Features

- Native `ulid` type with 16-byte storage
- Lexicographically sortable with timestamp ordering
- Base32 encoding (26-character strings)
- Monotonic generation for ordered inserts
- Full indexing and comparison support

## Installation

```bash
cargo pgrx install
```

## Quick Start

```sql
-- Generate ULIDs
SELECT gen_ulid();       -- Time-ordered ULID with a random tail
SELECT gen_ulid_mono();  -- Monotonic ULID per backend/thread

-- Use as primary key
CREATE TABLE users (
    id ulid PRIMARY KEY DEFAULT gen_ulid_mono(),
    name text
);

-- Insert some records
INSERT INTO users (name) VALUES ('alice'), ('bob'), ('charlie');

-- View records with timestamps
SELECT id, id::timestamptz as created_at, name FROM users;

-- Validate
SELECT ulid_is_valid('01JEPY8K5V3XQZW6M9N7P8Q2RT');
```

## Range Queries

ULIDs are naturally sortable by timestamp, making time-based queries efficient:

```sql
-- Query by time range (convert timestamp to ULID for index usage)
SELECT * FROM users
WHERE id >= '2025-01-01 00:00:00+00'::timestamptz::ulid
  AND id < '2026-01-01 00:00:00+00'::timestamptz::ulid;

-- Or using timestamp (without timezone)
SELECT * FROM users
WHERE id >= '2025-01-01'::timestamp::ulid
  AND id < '2026-01-01'::timestamp::ulid;

-- Last 24 hours
SELECT * FROM users
WHERE id >= (now() - interval '24 hours')::ulid;

-- Most recent records (ULIDs are naturally time-ordered)
SELECT * FROM users
ORDER BY id DESC
LIMIT 10;

-- Compare ULIDs directly
SELECT * FROM users
WHERE id > '01JEPY8K5V3XQZW6M9N7P8Q2RT'::ulid;
```

## Type Conversions

All casts require explicit `::` syntax:

```sql
'01JEPY8K5V3XQZW6M9N7P8Q2RT'::ulid  -- text to ulid
gen_ulid()::text                    -- ulid to text
now()::ulid                         -- timestamp to ulid
gen_ulid()::timestamptz             -- ulid to timestamp
```

## ULID Format

```
01AN4Z07BY79KA1307SR9X4MV3
|--------||--------------|
Timestamp   Randomness
 (10)         (16)
```

- 128 bits total (48-bit timestamp + 80-bit random)
- Sortable by creation time
- Millisecond precision

## Functions

- `gen_ulid()` - Generate random ULID
- `gen_ulid_mono()` - Generate monotonic ULID (maintains order within same
  millisecond)
- `ulid_is_valid(text)` - Validate ULID string

## ULID vs UUID (v7)

### gen_ulid()

- 48-bit millisecond timestamp + 80-bit random tail.
- Time-sorted in B-tree indexes, but not strictly increasing within the same
  millisecond.
- Uses a fast thread-local ChaCha-based RNG.

### gen_ulid_mono()

- Same 48+80 layout, but:
  - On each new millisecond, picks a random 80-bit starting value.
  - Within that millisecond, increments the 80-bit tail for each call.
- Result: IDs are strictly increasing per backend/thread within a millisecond,
  with very strong collision resistance and low overhead.

### Comparison with PostgreSQL uuidv7()

PostgreSQL's built-in uuidv7():

- Uses a 48-bit ms timestamp + 12-bit sub-ms fraction + ~62 random bits.
- Enforces per-backend monotonic timestamps via a high-resolution clock.
- Draws fresh random bits from the OS CSPRNG (`pg_strong_random`) on every call.

In spirit:

- `gen_ulid_mono()` is to ULIDs what `uuidv7()` is to UUIDs: both are
  per-backend monotonic, time-sortable ID generators, but `gen_ulid_mono()`
  keeps more random bits (80) and uses a cheap thread-local RNG, making it very
  fast and extremely collision-resistant for typical Postgres workloads, at the
  cost of finer time granularity. If you need sub-millisecond time ordering,
  PostgreSQL’s `uuidv7()` may be preferable.

## License

Licensed under either of:

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
  ([LICENSE-APACHE](LICENSE-APACHE))
- [MIT License](https://opensource.org/licenses/MIT)
  ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
