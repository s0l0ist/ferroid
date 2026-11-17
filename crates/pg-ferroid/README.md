# pg_ferroid

A PostgreSQL extension for [ULIDs](https://github.com/ulid/spec), built with
[ferroid](https://github.com/s0l0ist/ferroid/tree/main/crates/ferroid) and
[pgrx](https://github.com/pgcentralfoundation/pgrx).

The native `ulid` type stores 16 bytes, reads and writes 26-character Crockford
Base32 strings, and supports comparisons, B-tree indexes, and hash indexes.

## Getting Started

Requires Rust 1.95.0 or newer and PostgreSQL 15–18.

```sh
cargo install cargo-pgrx --version 0.20.0 --locked
cargo pgrx init --pg15=download
cd crates/pg-ferroid
cargo pgrx run pg15
```

This builds and installs the extension into a local PostgreSQL 15 instance and
opens `psql`. Load the extension with the SQL below.

## Quick Start

```sql
CREATE EXTENSION pg_ferroid;

CREATE TABLE users (
    id ulid PRIMARY KEY DEFAULT gen_ulid_mono(),
    name text
);

INSERT INTO users (name) VALUES ('alice'), ('bob'), ('charlie');

SELECT id, id::timestamptz AS created_at, name FROM users;
```

## Functions

| Function | Description |
| --- | --- |
| `gen_ulid()` | Generate a time-ordered ULID with a random tail. |
| `gen_ulid_mono()` | Generate strictly increasing ULIDs within one backend. |
| `ulid_is_valid(text)` | Check whether a string is a valid ULID. |

Monotonic ordering is per backend, not global across connections. If the clock
moves backwards, `gen_ulid_mono()` waits for it to catch up; query cancellation
and statement timeouts remain effective.

## Conversions

Use explicit casts:

| Cast | Result |
| --- | --- |
| `text::ulid` | Parse a Base32 ULID. |
| `ulid::text` | Encode a ULID as Base32. |
| `bytea::ulid` | Read 16 bytes in network byte order. |
| `ulid::bytea` | Return the 16-byte representation. |
| `timestamptz::ulid` | Set the timestamp and clear the random tail. |
| `ulid::timestamptz` | Extract the timestamp. |

Timestamp conversions have millisecond precision. Input timestamps must be
finite and within the ULID range, starting at the Unix epoch. For a
`timestamp without time zone`, specify the intended zone first:

```sql
SELECT (timestamp '2025-01-01 12:00:00' AT TIME ZONE 'UTC')::ulid;
```

## Range Queries

ULIDs sort by timestamp, so timestamp boundaries can use a ULID index:

```sql
SELECT * FROM users
WHERE id >= '2025-01-01 00:00:00+00'::timestamptz::ulid
  AND id < '2026-01-01 00:00:00+00'::timestamptz::ulid;

-- Last 24 hours
SELECT * FROM users
WHERE id >= (now() - interval '24 hours')::ulid;
```

## Development

Run the tests from `crates/pg-ferroid`:

```sh
cargo pgrx test pg15
```

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
