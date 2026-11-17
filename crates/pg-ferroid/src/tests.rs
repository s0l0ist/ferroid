use pgrx::{Json, datum::DatumWithOid, prelude::*};

use crate::ULID;

const _: () = {
    assert!(core::mem::size_of::<ULID>() == 16);
    assert!(core::mem::align_of::<ULID>() == 1);
};

struct Fixture {
    text: &'static str,
    hex: &'static str,
    utc: &'static str,
}

impl Fixture {
    fn bytes(&self) -> [u8; 16] {
        let mut bytes = [0; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&self.hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        bytes
    }

    fn timestamp_hex(&self) -> String {
        // The first six bytes are the timestamp; the remaining ten are randomness.
        format!("{}00000000000000000000", &self.hex[..12])
    }
}

// Ordered vectors include zero, a timestamp-byte carry, two IDs in the same
// millisecond, the PostgreSQL epoch, a typical ULID, and the maximum value.
const FIXTURES: &[Fixture] = &[
    Fixture {
        text: "00000000000000000000000000",
        hex: "00000000000000000000000000000000",
        utc: "1970-01-01 00:00:00",
    },
    Fixture {
        text: "000000007ZZZZZZZZZZZZZZZZZ",
        hex: "0000000000ffffffffffffffffffffff",
        utc: "1970-01-01 00:00:00.255",
    },
    Fixture {
        text: "00000000800000000000000000",
        hex: "00000000010000000000000000000000",
        utc: "1970-01-01 00:00:00.256",
    },
    Fixture {
        text: "00000000800000000000000001",
        hex: "00000000010000000000000000000001",
        utc: "1970-01-01 00:00:00.256",
    },
    Fixture {
        text: "00VHNCZB000000000000000000",
        hex: "00dc6acfac0000000000000000000000",
        utc: "2000-01-01 00:00:00",
    },
    Fixture {
        text: "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        hex: "01563e3ab5d3d6764c61efb99302bd5b",
        utc: "2016-07-30 23:54:10.259",
    },
    Fixture {
        text: "7ZZZZZZZZZZZZZZZZZZZZZZZZZ",
        hex: "ffffffffffffffffffffffffffffffff",
        utc: "10889-08-02 05:31:50.655",
    },
];

fn sql_literal(value: &str) -> String {
    Spi::get_one_with_args("SELECT quote_literal($1)", &[value.into()])
        .unwrap()
        .unwrap()
}

fn assert_sqlstate(statement: &str, expected: &str) {
    let statement = sql_literal(statement);
    Spi::run(&format!(
        "DO $test$
         BEGIN
             BEGIN
                 EXECUTE {statement};
                 RAISE EXCEPTION 'expected SQLSTATE {expected}';
             EXCEPTION WHEN SQLSTATE '{expected}' THEN
                 NULL;
             END;
         END $test$;"
    ))
    .unwrap();
}

fn assert_uses_index(query: &str, args: &[DatumWithOid<'_>], index_name: &str) {
    let plan = Spi::explain_with_args(query, args).unwrap();
    let mut nodes = vec![&plan.0[0]["Plan"]];
    while let Some(node) = nodes.pop() {
        if node["Index Name"].as_str() == Some(index_name) {
            return;
        }
        if let Some(children) = node["Plans"].as_array() {
            nodes.extend(children);
        }
    }
    panic!("expected index {index_name} in query plan: {plan:?}");
}

// Type and function registration.

#[pg_test]
fn type_storage_matches_the_fixed_size_representation() {
    let metadata = Spi::get_one_with_args::<Json>(
        "SELECT json_build_object(
             'length', typlen, 'alignment', typalign::text,
             'storage', typstorage::text, 'by_value', typbyval,
             'column_size', pg_column_size($1::ulid))
         FROM pg_type WHERE oid = 'ulid'::regtype",
        &[FIXTURES[0].text.into()],
    )
    .unwrap()
    .unwrap();

    assert_eq!(metadata.0["length"], 16);
    assert_eq!(metadata.0["alignment"], "c");
    assert_eq!(metadata.0["storage"], "p");
    assert_eq!(metadata.0["by_value"], false);
    assert_eq!(metadata.0["column_size"], 16);
}

#[pg_test]
fn functions_have_the_expected_sql_contracts() {
    let mismatches = Spi::get_one::<Vec<String>>(
        "WITH expected(signature, volatility) AS (VALUES
             ('gen_ulid()', 'v'), ('gen_ulid_mono()', 'v'),
             ('ulid_in(cstring)', 'i'), ('ulid_out(ulid)', 'i'),
             ('ulid_recv(internal)', 'i'), ('ulid_send(ulid)', 'i'),
             ('text_to_ulid(text)', 'i'), ('ulid_to_text(ulid)', 'i'),
             ('bytea_to_ulid(bytea)', 'i'), ('ulid_to_bytea(ulid)', 'i'),
             ('timestamptz_to_ulid(timestamptz)', 'i'),
             ('ulid_to_timestamptz(ulid)', 'i'), ('ulid_is_valid(text)', 'i'))
         SELECT array_agg(signature ORDER BY signature)
         FROM expected JOIN pg_proc ON oid = signature::regprocedure
         WHERE provolatile::text <> volatility OR NOT proisstrict
             OR proparallel::text <> 's'",
    )
    .unwrap();

    assert_eq!(mismatches, None);
}

// Text and binary I/O. Each public return path is exercised through SQL.

#[pg_test]
fn native_text_io_and_text_casts_preserve_known_values() {
    for fixture in FIXTURES {
        let (native, cast) = Spi::get_two_with_args::<String, String>(
            "SELECT ulid_out(ulid_in(textout($1)))::text, ($1::ulid)::text",
            &[fixture.text.into()],
        )
        .unwrap();

        assert_eq!(native.as_deref(), Some(fixture.text));
        assert_eq!(cast.as_deref(), Some(fixture.text));
    }
}

#[pg_test]
fn text_validation_and_errors_match_the_existing_decode_policy() {
    for text in [
        FIXTURES[5].text,
        FIXTURES[0].text,
        "ZZZZZZZZZZZZZZZZZZZZZZZZZZ",
    ] {
        assert_eq!(
            Spi::get_one_with_args::<bool>("SELECT ulid_is_valid($1)", &[text.into()]).unwrap(),
            Some(true),
            "{text}"
        );
    }
    for text in [
        "",
        "invalid-ulid",
        "01ARZ3NDEKTSV4RRFFQ69G5FA",
        "01ARZ3NDEKTSV4RRFFQ69G5FAVV",
        "01ARZ3NDEKTSV4RRFFQ69G5FAU",
    ] {
        assert_eq!(
            Spi::get_one_with_args::<bool>("SELECT ulid_is_valid($1)", &[text.into()]).unwrap(),
            Some(false),
            "{text}"
        );
    }
    for statement in ["SELECT 'invalid'::ulid", "SELECT 'invalid'::text::ulid"] {
        assert_sqlstate(statement, "22P02");
    }
}

#[pg_test]
fn bytea_casts_and_binary_send_match_known_network_bytes() {
    for fixture in FIXTURES {
        let (cast, send) = Spi::get_two_with_args::<String, String>(
            "SELECT encode($1::ulid::bytea, 'hex'), encode(ulid_send($1::ulid), 'hex')",
            &[fixture.text.into()],
        )
        .unwrap();
        assert_eq!(cast.as_deref(), Some(fixture.hex));
        assert_eq!(send.as_deref(), Some(fixture.hex));

        let restored = Spi::get_one_with_args::<String>(
            "SELECT (decode($1, 'hex')::ulid)::text",
            &[fixture.hex.into()],
        )
        .unwrap();
        assert_eq!(restored.as_deref(), Some(fixture.text));
    }
}

#[pg_test]
fn bytea_cast_rejects_incorrect_payload_lengths() {
    for length in [0, 15, 17] {
        assert_sqlstate(
            &format!("SELECT decode(repeat('00', {length}), 'hex')::ulid"),
            "22P03",
        );
    }
}

#[pg_test]
fn copy_binary_round_trip_preserves_known_values() {
    struct CopyFile(std::path::PathBuf);
    impl Drop for CopyFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let file = CopyFile(std::env::temp_dir().join(format!(
        "pg_ferroid_copy_{}_{suffix}.bin",
        std::process::id()
    )));
    let path = sql_literal(file.0.to_str().unwrap());

    Spi::run("CREATE TEMP TABLE wire_values (position int PRIMARY KEY, id ulid)").unwrap();
    for (index, fixture) in FIXTURES.iter().enumerate() {
        Spi::run_with_args(
            "INSERT INTO wire_values VALUES ($1, $2::ulid)",
            &[i32::try_from(index).unwrap().into(), fixture.text.into()],
        )
        .unwrap();
    }
    Spi::run(&format!("COPY wire_values TO {path} WITH (FORMAT binary)")).unwrap();
    Spi::run("TRUNCATE wire_values").unwrap();
    Spi::run(&format!(
        "COPY wire_values FROM {path} WITH (FORMAT binary)"
    ))
    .unwrap();
    let restored = Spi::get_one::<Vec<String>>(
        "SELECT array_agg(id::text ORDER BY position) FROM wire_values",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        restored,
        FIXTURES.iter().map(|f| f.text).collect::<Vec<_>>()
    );
}

// Timestamp conversions, including the unsigned range and explicit UTC use.

#[pg_test]
fn timestamp_casts_match_known_dates_and_clear_the_random_tail() {
    for fixture in FIXTURES {
        let utc = Spi::get_one_with_args::<String>(
            "SELECT ($1::ulid::timestamptz AT TIME ZONE 'UTC')::text",
            &[fixture.text.into()],
        )
        .unwrap();
        assert_eq!(utc.as_deref(), Some(fixture.utc));

        let boundary = Spi::get_one_with_args::<String>(
            "SELECT encode((($1::timestamp AT TIME ZONE 'UTC')::ulid)::bytea, 'hex')",
            &[fixture.utc.into()],
        )
        .unwrap();
        assert_eq!(boundary, Some(fixture.timestamp_hex()));
    }
}

#[pg_test]
fn timestamp_cast_truncates_submillisecond_precision_at_both_bounds() {
    for (timestamp, hex) in [
        (
            "1970-01-01 00:00:00.000999+00",
            "00000000000000000000000000000000",
        ),
        (
            "10889-08-02 05:31:50.655999+00",
            "ffffffffffff00000000000000000000",
        ),
    ] {
        let actual = Spi::get_one_with_args::<String>(
            "SELECT encode(($1::timestamptz::ulid)::bytea, 'hex')",
            &[timestamp.into()],
        )
        .unwrap();
        assert_eq!(actual.as_deref(), Some(hex));
    }
}

#[pg_test]
fn timestamp_cast_rejects_unrepresentable_dates_and_infinities() {
    for timestamp in [
        "1969-12-31 23:59:59.999999+00",
        "10889-08-02 05:31:50.656+00",
        "infinity",
        "-infinity",
    ] {
        assert_sqlstate(&format!("SELECT '{timestamp}'::timestamptz::ulid"), "22008");
    }
}

#[pg_test]
fn explicit_utc_conversions_ignore_the_session_timezone() {
    let fixture = &FIXTURES[5];
    for zone in ["UTC", "America/New_York", "Asia/Kathmandu"] {
        Spi::run_with_args("SELECT set_config('TimeZone', $1, true)", &[zone.into()]).unwrap();
        let (utc, boundary) = Spi::get_two_with_args::<String, String>(
            "SELECT ($1::ulid::timestamptz AT TIME ZONE 'UTC')::text,
                 encode((($2::timestamp AT TIME ZONE 'UTC')::ulid)::bytea, 'hex')",
            &[fixture.text.into(), fixture.utc.into()],
        )
        .unwrap();
        assert_eq!(utc.as_deref(), Some(fixture.utc));
        assert_eq!(boundary, Some(fixture.timestamp_hex()));
    }
}

// Generator wiring. These are the only tests that generate new identifiers.

#[pg_test]
fn generators_return_non_null_ids_with_timestamps_through_sql() {
    for query in ["SELECT gen_ulid()", "SELECT gen_ulid_mono()"] {
        let id = Spi::get_one::<ULID>(query).unwrap().expect(query);
        assert!(id.timestamp() > 0, "{query}");
    }
}

#[pg_test]
fn monotonic_generation_increases_across_ordered_calls_in_one_backend() {
    let mut previous = None;
    for _ in 0..64 {
        let id = Spi::get_one::<ULID>("SELECT gen_ulid_mono()")
            .unwrap()
            .unwrap();
        if let Some(previous) = previous {
            assert!(previous < id);
        }
        previous = Some(id);
    }
}

// Storage, operators, and operator classes.

#[pg_test]
fn stored_rows_preserve_multiple_columns_and_nulls() {
    Spi::run("CREATE TEMP TABLE stored_values (position int PRIMARY KEY, a ulid, b ulid)").unwrap();
    for (index, fixture) in FIXTURES.iter().enumerate() {
        let other = &FIXTURES[FIXTURES.len() - index - 1];
        let position = i32::try_from(index).unwrap();
        Spi::run_with_args(
            "INSERT INTO stored_values VALUES ($1, $2::ulid, $3::ulid)",
            &[position.into(), fixture.text.into(), other.text.into()],
        )
        .unwrap();
        // get_two returns owned values after its SPI memory context has closed.
        let (a, b) = Spi::get_two_with_args::<ULID, ULID>(
            "SELECT a, b FROM stored_values WHERE position = $1",
            &[position.into()],
        )
        .unwrap();
        assert_eq!(a.map(|id| *id.as_bytes()), Some(fixture.bytes()));
        assert_eq!(b.map(|id| *id.as_bytes()), Some(other.bytes()));
    }
    Spi::run("INSERT INTO stored_values VALUES (99, NULL, NULL)").unwrap();
    assert_eq!(
        Spi::get_two::<ULID, ULID>("SELECT a, b FROM stored_values WHERE position = 99").unwrap(),
        (None, None)
    );
}

#[pg_test]
fn sql_comparisons_follow_timestamp_then_randomness_order() {
    for (i, left) in FIXTURES.iter().enumerate() {
        for (j, right) in FIXTURES.iter().enumerate() {
            let actual = Spi::get_one_with_args::<Vec<bool>>(
                "SELECT ARRAY[$1::ulid = $2::ulid, $1::ulid <> $2::ulid,
                     $1::ulid < $2::ulid, $1::ulid <= $2::ulid,
                     $1::ulid > $2::ulid, $1::ulid >= $2::ulid]",
                &[left.text.into(), right.text.into()],
            )
            .unwrap();
            assert_eq!(
                actual,
                Some(vec![i == j, i != j, i < j, i <= j, i > j, i >= j])
            );
        }
    }
}

#[pg_test]
fn btree_index_supports_ordering_ranges_and_uniqueness() {
    Spi::run("CREATE TEMP TABLE ordered_values (id ulid PRIMARY KEY)").unwrap();
    for fixture in FIXTURES.iter().rev() {
        Spi::run_with_args(
            "INSERT INTO ordered_values VALUES ($1::ulid)",
            &[fixture.text.into()],
        )
        .unwrap();
    }
    assert_sqlstate(
        "INSERT INTO ordered_values VALUES ('00000000000000000000000000')",
        "23505",
    );
    Spi::run("SET LOCAL enable_seqscan = off").unwrap();
    assert_uses_index(
        "SELECT id FROM ordered_values ORDER BY id",
        &[],
        "ordered_values_pkey",
    );
    let ordered =
        Spi::get_one::<Vec<String>>("SELECT array_agg(id::text ORDER BY id) FROM ordered_values")
            .unwrap()
            .unwrap();
    assert_eq!(ordered, FIXTURES.iter().map(|f| f.text).collect::<Vec<_>>());

    let predicate = "id >= '1970-01-01 00:00:00.255+00'::timestamptz::ulid
                     AND id < '1970-01-01 00:00:00.257+00'::timestamptz::ulid";
    assert_uses_index(
        &format!("SELECT id FROM ordered_values WHERE {predicate} ORDER BY id"),
        &[],
        "ordered_values_pkey",
    );
    let range = Spi::get_one::<Vec<String>>(&format!(
        "SELECT array_agg(id::text ORDER BY id) FROM ordered_values WHERE {predicate}"
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        range,
        FIXTURES[1..4].iter().map(|f| f.text).collect::<Vec<_>>()
    );
}

#[pg_test]
fn hash_index_finds_exact_matches_and_missing_keys() {
    Spi::run("CREATE TEMP TABLE hash_values (id ulid); CREATE INDEX hash_values_idx ON hash_values USING hash(id)").unwrap();
    for fixture in FIXTURES.iter().chain([&FIXTURES[5]]) {
        Spi::run_with_args(
            "INSERT INTO hash_values VALUES ($1::ulid)",
            &[fixture.text.into()],
        )
        .unwrap();
    }
    Spi::run("SET LOCAL enable_seqscan = off").unwrap();
    let args = [FIXTURES[5].text.into()];
    assert_uses_index(
        "SELECT id FROM hash_values WHERE id = $1::ulid",
        &args,
        "hash_values_idx",
    );
    assert_eq!(
        Spi::get_one_with_args::<i64>(
            "SELECT count(*) FROM hash_values WHERE id = $1::ulid",
            &args
        )
        .unwrap(),
        Some(2)
    );
    assert_eq!(
        Spi::get_one::<i64>(
            "SELECT count(*) FROM hash_values WHERE id = '00000000000000000000000002'::ulid"
        )
        .unwrap(),
        Some(0)
    );
}

#[pg_test]
fn grouping_and_distinct_agree_on_equal_values() {
    let args = [
        FIXTURES[0].text.into(),
        FIXTURES[5].text.into(),
        FIXTURES[6].text.into(),
    ];
    let (ids, counts) = Spi::get_two_with_args::<Vec<String>, Vec<i64>>(
        "WITH input(id) AS (VALUES ($1::ulid), ($1::ulid),
             ($2::ulid), ($2::ulid), ($2::ulid), ($3::ulid)),
         grouped AS (SELECT id, count(*) AS n FROM input GROUP BY id)
         SELECT array_agg(id::text ORDER BY id), array_agg(n ORDER BY id) FROM grouped",
        &args,
    )
    .unwrap();
    assert_eq!(
        ids,
        Some(vec![
            FIXTURES[0].text.into(),
            FIXTURES[5].text.into(),
            FIXTURES[6].text.into()
        ])
    );
    assert_eq!(counts, Some(vec![2, 3, 1]));
    assert_eq!(
        Spi::get_one_with_args::<i64>(
            "SELECT count(DISTINCT id) FROM (VALUES ($1::ulid), ($1::ulid), ($2::ulid), ($3::ulid)) AS input(id)",
            &args,
        )
        .unwrap(),
        Some(3)
    );
}
