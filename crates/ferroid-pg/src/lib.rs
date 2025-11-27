use ferroid::{
    base32::Base32UlidExt,
    generator::thread_local::Ulid,
    id::{BeBytes, Id, ULID as InnerULID},
};
use pgrx::{
    callconv::{ArgAbi, BoxRet},
    datum::{FromDatum, IntoDatum},
    pg_sys,
    pgrx_sql_entity_graph::metadata::{
        ArgumentError, Returns, ReturnsError, SqlMapping, SqlTranslatable,
    },
    prelude::*,
    rust_regtypein, PgMemoryContexts, StringInfo,
};

pgrx::pg_module_magic!();

// ============================================================================
// ULID
// ============================================================================

type Bytes = <<InnerULID as Id>::Ty as BeBytes>::ByteArray;

/// A PostgreSQL ULID type backed by `ferroid`.
///
/// Represents a 128-bit, lexicographically sortable identifier with
/// timestamp-first ordering.
///
/// Storage characteristics:
/// - Fixed width: 16 bytes
/// - Representation: big-endian binary ULID
/// - Passed by reference (not by value)
/// - Optimized for B-tree indexing by creation time
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, PostgresHash, PostgresEq, PostgresOrd,
)]
#[repr(transparent)]
pub struct ULID {
    bytes: Bytes,
}

impl ULID {
    #[inline(always)]
    pub const fn from_bytes(bytes: Bytes) -> Self {
        Self { bytes }
    }

    #[inline(always)]
    pub const fn as_bytes(&self) -> &Bytes {
        &self.bytes
    }

    #[inline(always)]
    fn to_ulid(&self) -> InnerULID {
        InnerULID::from_raw(<<InnerULID as Id>::Ty>::from_be_bytes(self.bytes))
    }

    #[inline(always)]
    fn from_ulid(ulid: InnerULID) -> Self {
        Self::from_bytes(ulid.to_raw().to_be_bytes())
    }

    #[inline(always)]
    fn timestamp(&self) -> i64 {
        self.to_ulid().timestamp() as i64
    }
}

impl From<InnerULID> for ULID {
    #[inline(always)]
    fn from(ulid: InnerULID) -> Self {
        Self::from_ulid(ulid)
    }
}

impl From<&ULID> for InnerULID {
    #[inline(always)]
    fn from(p: &ULID) -> Self {
        p.to_ulid()
    }
}

impl From<ULID> for InnerULID {
    #[inline(always)]
    fn from(p: ULID) -> Self {
        p.to_ulid()
    }
}

impl core::fmt::Display for ULID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.to_ulid().encode().fmt(f)
    }
}

// ============================================================================
// DATUM + CALLCONV GLUE
// ============================================================================

impl FromDatum for ULID {
    unsafe fn from_polymorphic_datum(
        datum: pg_sys::Datum,
        is_null: bool,
        _typoid: pg_sys::Oid,
    ) -> Option<Self> {
        if is_null {
            return None;
        }

        // SAFETY:
        // - `ulid` is defined with INTERNALLENGTH = 16 and STORAGE = plain, so
        //   Postgres stores exactly `size_of::<Bytes>()` bytes here.
        // - `Bytes` is `[u8; 16]` with alignment 1, so this cast is sound.
        // - We copy into a local `Bytes` and do not retain a pointer into
        //   Postgres-managed memory.
        let ptr = datum.cast_mut_ptr::<u8>() as *const Bytes;
        Some(ULID::from_bytes(*ptr))
    }
}

impl IntoDatum for ULID {
    fn into_datum(self) -> Option<pg_sys::Datum> {
        // Allocate space for a single `Bytes` in the current memory context.
        let raw: *mut Bytes = unsafe {
            // SAFETY:
            // - `CurrentMemoryContext` is a valid Postgres MemoryContext.
            // - `palloc_struct::<Bytes>()` allocates exactly
            //   `size_of::<Bytes>()` bytes and is aligned.
            PgMemoryContexts::CurrentMemoryContext.palloc_struct::<Bytes>()
        };

        unsafe {
            // SAFETY:
            // - `raw` points to uninitialized but valid memory for one `Bytes`.
            // - We fully initialize it with `self.bytes` before exposing it.
            *raw = self.bytes;
        }

        // Postgres expects a pointer to the first byte as the Datum.
        Some((raw as *mut u8).into())
    }

    fn type_oid() -> pg_sys::Oid {
        // Look up the OID of the 'ulid' type once it exists
        rust_regtypein::<Self>()
    }
}

unsafe impl<'fcx> ArgAbi<'fcx> for ULID
where
    Self: 'fcx,
{
    unsafe fn unbox_arg_unchecked(arg: ::pgrx::callconv::Arg<'_, 'fcx>) -> Self {
        unsafe {
            arg.unbox_arg_using_from_datum()
                .unwrap_or_else(|| pgrx::error!("ULID argument must not be NULL"))
        }
    }
}

unsafe impl BoxRet for ULID {
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        match self.into_datum() {
            Some(datum) => unsafe { fcinfo.return_raw_datum(datum) },
            None => fcinfo.return_null(),
        }
    }
}

unsafe impl SqlTranslatable for ULID {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As("ulid".into()))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As("ulid".into())))
    }
}

// ============================================================================
// TEXT I/O (Base32 encoding/decoding)
// ============================================================================
#[pg_extern(immutable, parallel_safe, strict, requires = ["shell_type"])]
fn ulid_in(input: &core::ffi::CStr) -> ULID {
    InnerULID::decode(input.to_bytes())
        .map(ULID::from_ulid)
        .unwrap_or_else(|e| pgrx::error!("invalid ULID: {}", e))
}

#[pg_extern(immutable, parallel_safe, strict, requires = ["shell_type"])]
fn ulid_out(ulid: ULID) -> &'static core::ffi::CStr {
    let encoded = ulid.to_ulid().encode();
    let bytes = encoded.as_bytes();
    let len = bytes
        .len()
        .try_into()
        .unwrap_or_else(|e| pgrx::error!("ULID base32 length overflowed i32: {}", e));

    let mut s = StringInfo::with_capacity(len);
    s.push_bytes(bytes);
    // SAFETY:
    // - `s` was created via `StringInfo::with_capacity` and only modified with
    //   `push_bytes`, which maintains the internal trailing NUL and correct
    //   len.
    // - `encode().as_bytes()` yields only non-NUL ASCII bytes, so there are no
    //   interior NULs.
    // - `leak_cstr` docs state this combination upholds
    //   `CStr::from_bytes_with_nul` invariants; Postgres owns and frees the
    //   underlying memory.
    unsafe { s.leak_cstr() }
}

// ============================================================================
// ULID GENERATION
// ============================================================================

/// Generate a new monotonic ULID
///
/// Monotonic ULIDs guarantee ordering within the same millisecond
#[pg_extern(strict, parallel_safe)]
fn gen_ulid_mono() -> ULID {
    ULID::from_ulid(Ulid::new_ulid_mono())
}

/// Generate a new random ULID (non-monotonic)
#[pg_extern(strict, parallel_safe)]
fn gen_ulid() -> ULID {
    ULID::from_ulid(Ulid::new_ulid())
}

// ============================================================================
// CASTING SUPPORT
// ============================================================================

// PostgreSQL epoch: 2000-01-01 00:00:00 UTC Unix epoch: 1970-01-01 00:00:00 UTC
// Difference: 946684800 seconds = 946684800000000 microseconds
const PG_EPOCH_OFFSET_MICROS: i64 = 946_684_800_000_000;

/// Cast ULID to timestamptz (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_timestamptz(ulid: ULID) -> TimestampWithTimeZone {
    let ms = ulid.to_ulid().timestamp() as i64;
    let unix_micros = ms.saturating_mul(1_000);
    let pg_micros = unix_micros.saturating_sub(PG_EPOCH_OFFSET_MICROS);

    TimestampWithTimeZone::try_from(pg_micros)
        .unwrap_or_else(|e| pgrx::error!("timestamp out of range: {}", e))
}

/// Cast timestamptz to ULID (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn timestamptz_to_ulid(ts: TimestampWithTimeZone) -> ULID {
    let pg_micros: i64 = ts
        .try_into()
        .unwrap_or_else(|e| pgrx::error!("invalid timestamp: {}", e));

    let unix_micros = pg_micros.saturating_add(PG_EPOCH_OFFSET_MICROS);
    let ms = (unix_micros / 1_000) as u64;

    ULID::from_ulid(InnerULID::from_timestamp(ms as u128))
}

/// Cast ULID to timestamp (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_timestamp(ulid: ULID) -> Timestamp {
    ulid_to_timestamptz(ulid).into()
}

/// Cast timestamp to ULID (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn timestamp_to_ulid(ts: Timestamp) -> ULID {
    timestamptz_to_ulid(ts.into())
}

/// Cast text to ULID (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn text_to_ulid(text: &str) -> ULID {
    InnerULID::decode(text)
        .map(ULID::from_ulid)
        .unwrap_or_else(|e| pgrx::error!("invalid ULID text: {}", e))
}

/// Cast ULID to text (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_text(ulid: ULID) -> String {
    ulid.to_ulid().encode().as_string()
}

/// Cast bytea to ULID (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn bytea_to_ulid(bytes: &[u8]) -> ULID {
    let arr: Bytes = bytes.try_into().unwrap_or_else(|_| {
        pgrx::error!(
            "invalid bytea length for ulid: expected {} bytes, got {}",
            <<InnerULID as Id>::Ty as BeBytes>::SIZE,
            bytes.len()
        )
    });
    ULID::from_bytes(arr)
}

/// Cast ULID to bytea (requires explicit cast)
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_bytea(ulid: ULID) -> Vec<u8> {
    ulid.as_bytes().to_vec()
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Check if a string is a valid ULID
#[pg_extern(immutable, parallel_safe, strict)]
fn ulid_is_valid(text: &str) -> bool {
    InnerULID::decode(text.as_bytes()).is_ok()
}

// ============================================================================
// SQL TYPE CREATION
// ============================================================================
extension_sql!(r#"CREATE TYPE ulid;"#, name = "shell_type", bootstrap);
extension_sql!(
    r#"
CREATE TYPE ulid (
    INPUT = ulid_in,
    OUTPUT = ulid_out,
    INTERNALLENGTH = 16,
    ALIGNMENT = char,
    STORAGE = plain,
    PASSEDBYVALUE = false
);
"#,
    name = "concrete_type",
    creates = [Type(ULID)],
    requires = ["shell_type", ulid_in, ulid_out]
);
extension_sql!(
    r#"
COMMENT ON TYPE ulid IS 'Universally Unique Lexicographically Sortable Identifier - 128-bit identifier with timestamp ordering';
COMMENT ON FUNCTION gen_ulid() IS 'Generate a new random ULID';
COMMENT ON FUNCTION gen_ulid_mono() IS 'Generate a new monotonic ULID (maintains ordering within same millisecond)';
COMMENT ON FUNCTION ulid_is_valid(text) IS 'Check if a text string is a valid ULID';
"#,
    name = "add_comments",
    requires = ["concrete_type", gen_ulid, gen_ulid_mono, ulid_is_valid]
);

// ============================================================================
// TESTS
// ============================================================================

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use super::*;

    // ========================================================================
    // Core Type Tests
    // ========================================================================

    /// Verify ULID has correct type properties in PostgreSQL catalog
    #[pg_test]
    fn type_properties() {
        assert_eq!(
            core::mem::size_of::<ULID>(),
            16,
            "Rust size should be 16 bytes"
        );

        let typlen = Spi::get_one::<i16>("SELECT typlen FROM pg_type WHERE typname = 'ulid'")
            .unwrap()
            .unwrap();
        assert_eq!(typlen, 16, "PostgreSQL INTERNALLENGTH should be 16");

        let typalign =
            Spi::get_one::<String>("SELECT typalign::text FROM pg_type WHERE typname = 'ulid'")
                .unwrap()
                .unwrap();
        assert_eq!(typalign, "c", "Alignment should be char");

        let typstorage =
            Spi::get_one::<String>("SELECT typstorage::text FROM pg_type WHERE typname = 'ulid'")
                .unwrap()
                .unwrap();
        assert_eq!(typstorage, "p", "Storage should be plain");

        let typbyval = Spi::get_one::<bool>("SELECT typbyval FROM pg_type WHERE typname = 'ulid'")
            .unwrap()
            .unwrap();
        assert!(!typbyval, "Should be passed by reference");
    }

    /// Verify storage is exactly 16 bytes (no varlena header)
    #[pg_test]
    fn fixed_size_storage() {
        let ulid = gen_ulid();
        let size = Spi::get_one::<i32>(&format!("SELECT pg_column_size('{}'::ulid)", ulid))
            .unwrap()
            .unwrap();
        assert_eq!(size, 16, "Storage must be exactly 16 bytes");
    }

    // ========================================================================
    // Generation Tests
    // ========================================================================

    /// Verify basic ULID generation works
    #[pg_test]
    fn generation_basic() {
        let ulid1 = gen_ulid();
        let ulid2 = gen_ulid();
        assert_ne!(ulid1, ulid2, "Random ULIDs should differ");
    }

    /// Verify monotonic ULIDs maintain strict ordering
    #[pg_test]
    fn generation_monotonic_ordering() {
        let ulid1 = gen_ulid_mono();
        let ulid2 = gen_ulid_mono();
        let ulid3 = gen_ulid_mono();
        assert!(
            ulid1 < ulid2 && ulid2 < ulid3,
            "Monotonic ULIDs must be strictly ordered"
        );
    }

    /// Verify generation from SQL
    #[pg_test]
    fn generation_from_sql() {
        let ulid = Spi::get_one::<ULID>("SELECT gen_ulid()").unwrap().unwrap();
        assert!(
            ulid.timestamp() > 0,
            "Generated ULID should have valid timestamp"
        );
    }

    /// Verify rapid monotonic generation maintains ordering
    #[pg_test]
    fn generation_monotonic_rapid() {
        let mut ulids = Vec::new();
        for _ in 0..100 {
            ulids.push(gen_ulid_mono());
        }
        for i in 1..ulids.len() {
            assert!(
                ulids[i - 1] < ulids[i],
                "All monotonic ULIDs must be strictly ordered"
            );
        }
    }

    // ========================================================================
    // Text I/O Tests
    // ========================================================================

    /// Verify round-trip through text representation
    #[pg_test]
    fn text_io_round_trip() {
        let ulid = gen_ulid();
        let text = ulid_to_text(ulid);
        let parsed = text_to_ulid(&text);
        assert_eq!(ulid, parsed, "ULID should survive text round-trip");
    }

    /// Verify text I/O via SQL casting
    #[pg_test]
    fn text_io_via_sql() {
        let ulid = gen_ulid();

        let parsed = Spi::get_one::<ULID>(&format!("SELECT '{}'::ulid", ulid))
            .unwrap()
            .unwrap();
        assert_eq!(ulid, parsed);

        let text_back = Spi::get_one::<String>(&format!("SELECT '{}'::ulid::text", ulid))
            .unwrap()
            .unwrap();
        assert_eq!(ulid_to_text(ulid), text_back);
    }

    /// Verify valid ULID formats are accepted
    #[pg_test]
    fn validation_valid_inputs() {
        assert!(ulid_is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAV"), "Standard ULID");
        assert!(ulid_is_valid("00000000000000000000000000"), "All zeros");
        assert!(ulid_is_valid("ZZZZZZZZZZZZZZZZZZZZZZZZZZ"), "Max enc value");
    }

    /// Verify invalid ULID formats are rejected
    #[pg_test]
    fn validation_invalid_inputs() {
        assert!(!ulid_is_valid(""), "Empty string");
        assert!(!ulid_is_valid("invalid-ulid"), "Invalid characters");
        assert!(!ulid_is_valid("01ARZ3NDEKTSV4RRFFQ69G5FA"), "Too short");
        assert!(!ulid_is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAVV"), "Too long");
        assert!(
            !ulid_is_valid("01ARZ3NDEKTSV4RRFFQ69G5FAU"),
            "Invalid character 'U'"
        );
    }

    /// Verify parsing invalid ULID throws error
    #[pg_test]
    #[should_panic(expected = "invalid ULID")]
    fn validation_parse_error() {
        let _ = Spi::get_one::<ULID>("SELECT 'invalid'::ulid");
    }

    // ========================================================================
    // Comparison Operator Tests
    // ========================================================================

    /// Verify Rust-level comparison operators
    #[pg_test]
    fn comparison_rust_operators() {
        let low = ULID::from_ulid(InnerULID::from_timestamp(1000));
        let high = ULID::from_ulid(InnerULID::from_timestamp(2000));

        assert_eq!(low, low);
        assert_ne!(low, high);
        assert!(low < high);
        assert!(low <= high);
        assert!(high > low);
        assert!(high >= low);
    }

    /// Verify SQL comparison operators
    #[pg_test]
    fn comparison_sql_operators() {
        let low = gen_ulid_mono();
        std::thread::sleep(core::time::Duration::from_millis(10));
        let high = gen_ulid_mono();

        assert!(
            Spi::get_one::<bool>(&format!("SELECT '{}'::ulid < '{}'::ulid", low, high))
                .unwrap()
                .unwrap()
        );
        assert!(
            Spi::get_one::<bool>(&format!("SELECT '{}'::ulid <= '{}'::ulid", low, high))
                .unwrap()
                .unwrap()
        );
        assert!(
            Spi::get_one::<bool>(&format!("SELECT '{}'::ulid = '{}'::ulid", low, low))
                .unwrap()
                .unwrap()
        );
        assert!(
            Spi::get_one::<bool>(&format!("SELECT '{}'::ulid <> '{}'::ulid", low, high))
                .unwrap()
                .unwrap()
        );
        assert!(
            Spi::get_one::<bool>(&format!("SELECT '{}'::ulid > '{}'::ulid", high, low))
                .unwrap()
                .unwrap()
        );
        assert!(
            Spi::get_one::<bool>(&format!("SELECT '{}'::ulid >= '{}'::ulid", high, low))
                .unwrap()
                .unwrap()
        );
    }

    // ========================================================================
    // Timestamp Tests
    // ========================================================================

    /// Verify timestamp extraction returns reasonable values
    #[pg_test]
    fn timestamp_extraction() {
        let ulid = gen_ulid();
        let ms = ulid.timestamp();
        assert!(ms > 1_600_000_000_000, "Should be after Sept 2020");
        assert!(ms < 2_000_000_000_000, "Should be before May 2033");
    }

    /// Verify round-trip through timestamptz
    #[pg_test]
    fn timestamp_timestamptz_round_trip() {
        let ulid = gen_ulid();
        let ts = ulid_to_timestamptz(ulid);
        let ulid2 = timestamptz_to_ulid(ts);

        let ms1 = ulid.timestamp();
        let ms2 = ulid2.timestamp();
        assert!(
            (ms1 - ms2).abs() <= 1,
            "Should be within same millisecond: {}",
            (ms1 - ms2).abs()
        );
    }

    /// Verify round-trip through timestamp
    #[pg_test]
    fn timestamp_timestamp_round_trip() {
        let ulid = gen_ulid();
        let ts = ulid_to_timestamp(ulid);
        let ulid2 = timestamp_to_ulid(ts);

        let ms1 = ulid.timestamp();
        let ms2 = ulid2.timestamp();
        assert!(
            (ms1 - ms2).abs() <= 1,
            "Should be within same millisecond: {}",
            (ms1 - ms2).abs()
        );
    }

    /// Verify ULID from known timestamp
    #[pg_test]
    fn timestamp_from_known_value() {
        let ts =
            Spi::get_one::<TimestampWithTimeZone>("SELECT '2024-01-01 00:00:00+00'::timestamptz")
                .unwrap()
                .unwrap();
        let ulid = timestamptz_to_ulid(ts);
        let ms = ulid.timestamp();
        assert_eq!(ms, 1704067200000, "Jan 1, 2024 00:00:00 UTC");
    }

    /// Verify timestamp ordering
    #[pg_test]
    fn timestamp_ordering() {
        let ts1 = Spi::get_one::<TimestampWithTimeZone>("SELECT '2024-01-01'::timestamptz")
            .unwrap()
            .unwrap();
        let ts2 = Spi::get_one::<TimestampWithTimeZone>("SELECT '2024-12-31'::timestamptz")
            .unwrap()
            .unwrap();
        let ulid1 = timestamptz_to_ulid(ts1);
        let ulid2 = timestamptz_to_ulid(ts2);
        assert!(
            ulid1 < ulid2,
            "Earlier timestamp should produce smaller ULID"
        );
    }

    /// Verify edge case: epoch timestamp
    #[pg_test]
    fn timestamp_epoch() {
        let ulid_zero = ULID::from_ulid(InnerULID::from_timestamp(0));
        assert_eq!(ulid_zero.timestamp(), 0);

        let ulid_small = ULID::from_ulid(InnerULID::from_timestamp(1000));
        assert_eq!(ulid_small.timestamp(), 1000);
    }

    // ========================================================================
    // Cast Tests
    // ========================================================================

    /// Verify text casting
    #[pg_test]
    fn cast_text_round_trip() {
        let ulid = gen_ulid();

        let result = Spi::get_one::<String>(&format!("SELECT '{}'::ulid::text", ulid))
            .unwrap()
            .unwrap();
        assert_eq!(ulid_to_text(ulid), result);

        let ulid_back = Spi::get_one::<ULID>(&format!("SELECT '{}'::text::ulid", ulid))
            .unwrap()
            .unwrap();
        assert_eq!(ulid, ulid_back);
    }

    /// Verify bytea casting
    #[pg_test]
    fn cast_bytea_round_trip() {
        let ulid = gen_ulid();
        let bytes = ulid_to_bytea(ulid);
        assert_eq!(bytes.len(), 16, "Bytea should be 16 bytes");

        let ulid_back = bytea_to_ulid(&bytes);
        assert_eq!(ulid, ulid_back, "ULID should survive bytea round-trip");
    }

    // ========================================================================
    // Storage & Indexing Tests
    // ========================================================================

    /// Verify ULIDs can be stored and retrieved from tables
    #[pg_test]
    fn storage_table_round_trip() {
        let original = gen_ulid();

        Spi::run("CREATE TEMP TABLE ulid_test (id ulid)").unwrap();
        Spi::run(&format!(
            "INSERT INTO ulid_test VALUES ('{}'::ulid)",
            original
        ))
        .unwrap();

        let retrieved = Spi::get_one::<ULID>("SELECT id FROM ulid_test")
            .unwrap()
            .unwrap();
        assert_eq!(
            original, retrieved,
            "ULID should survive storage round-trip"
        );
    }

    /// Verify B-tree index maintains correct ordering
    #[pg_test]
    fn storage_btree_index() {
        let ulid1 = gen_ulid_mono();
        let ulid2 = gen_ulid_mono();
        let ulid3 = gen_ulid_mono();

        Spi::run("CREATE TEMP TABLE ulid_indexed (id ulid PRIMARY KEY)").unwrap();

        // Insert in non-sorted order
        Spi::run(&format!(
            "INSERT INTO ulid_indexed VALUES ('{}'::ulid), ('{}'::ulid), ('{}'::ulid)",
            ulid2, ulid3, ulid1
        ))
        .unwrap();

        // Verify ORDER BY returns correct order
        let first = Spi::get_one::<ULID>("SELECT id FROM ulid_indexed ORDER BY id LIMIT 1")
            .unwrap()
            .unwrap();
        let last = Spi::get_one::<ULID>("SELECT id FROM ulid_indexed ORDER BY id DESC LIMIT 1")
            .unwrap()
            .unwrap();

        assert_eq!(first, ulid1, "Smallest ULID should come first");
        assert_eq!(last, ulid3, "Largest ULID should come last");
    }

    /// Verify hash index supports equality lookups
    #[pg_test]
    fn storage_hash_index() {
        let ulid = gen_ulid();

        Spi::run("CREATE TEMP TABLE ulid_hash (id ulid)").unwrap();
        Spi::run("CREATE INDEX ulid_hash_idx ON ulid_hash USING hash(id)").unwrap();
        Spi::run(&format!("INSERT INTO ulid_hash VALUES ('{}'::ulid)", ulid)).unwrap();

        let found = Spi::get_one::<bool>(&format!(
            "SELECT EXISTS(SELECT 1 FROM ulid_hash WHERE id = '{}'::ulid)",
            ulid
        ))
        .unwrap()
        .unwrap();

        assert!(found, "Hash index should find inserted ULID");
    }

    /// Verify NULL values are handled correctly
    #[pg_test]
    fn storage_null_handling() {
        Spi::run("CREATE TEMP TABLE ulid_nullable (id ulid)").unwrap();
        Spi::run("INSERT INTO ulid_nullable VALUES (NULL)").unwrap();

        let result = Spi::get_one::<ULID>("SELECT id FROM ulid_nullable").unwrap();
        assert!(result.is_none(), "NULL should be returned as None");
    }

    /// Verify multiple ULID columns work correctly
    #[pg_test]
    fn storage_multiple_columns() {
        let ulid1 = gen_ulid();
        let ulid2 = gen_ulid();

        Spi::run("CREATE TEMP TABLE multi_ulid (a ulid, b ulid)").unwrap();
        Spi::run(&format!(
            "INSERT INTO multi_ulid VALUES ('{}'::ulid, '{}'::ulid)",
            ulid1, ulid2
        ))
        .unwrap();

        let retrieved_a = Spi::get_one::<ULID>("SELECT a FROM multi_ulid")
            .unwrap()
            .unwrap();
        let retrieved_b = Spi::get_one::<ULID>("SELECT b FROM multi_ulid")
            .unwrap()
            .unwrap();

        assert_eq!(ulid1, retrieved_a, "First column should match");
        assert_eq!(ulid2, retrieved_b, "Second column should match");
    }

    // ========================================================================
    // Range Query Tests
    // ========================================================================

    /// Verify basic range queries with known timestamps
    #[pg_test]
    fn range_query_by_timestamp() {
        let ts1 =
            Spi::get_one::<TimestampWithTimeZone>("SELECT '2024-01-01 10:00:00+00'::timestamptz")
                .unwrap()
                .unwrap();
        let ts2 =
            Spi::get_one::<TimestampWithTimeZone>("SELECT '2024-01-01 12:00:00+00'::timestamptz")
                .unwrap()
                .unwrap();
        let ts3 =
            Spi::get_one::<TimestampWithTimeZone>("SELECT '2024-01-01 14:00:00+00'::timestamptz")
                .unwrap()
                .unwrap();

        let ulid1 = timestamptz_to_ulid(ts1);
        let ulid2 = timestamptz_to_ulid(ts2);
        let ulid3 = timestamptz_to_ulid(ts3);

        Spi::run("CREATE TEMP TABLE events (id ulid PRIMARY KEY)").unwrap();
        Spi::run(&format!(
            "INSERT INTO events VALUES ('{}'::ulid), ('{}'::ulid), ('{}'::ulid)",
            ulid1, ulid2, ulid3
        ))
        .unwrap();

        // Query events between 10am and 2pm (exclusive)
        let count = Spi::get_one::<i64>(&format!(
            "SELECT COUNT(*) FROM events WHERE id >= '{}'::ulid AND id < '{}'::ulid",
            ulid1, ulid3
        ))
        .unwrap()
        .unwrap();

        assert_eq!(count, 2, "Should find events at 10am and 12pm");
    }

    /// Verify filtering events within a time window
    #[pg_test]
    fn range_query_time_window() {
        let mut ulids = Vec::new();
        for _ in 0..10 {
            ulids.push(gen_ulid_mono());
        }

        Spi::run("CREATE TEMP TABLE recent_events (id ulid PRIMARY KEY)").unwrap();
        for ulid in &ulids {
            Spi::run(&format!(
                "INSERT INTO recent_events VALUES ('{}'::ulid)",
                ulid
            ))
            .unwrap();
        }

        let oldest = ulids[0];
        let newest = ulids[ulids.len() - 1];

        // Get all events in range (inclusive)
        let count = Spi::get_one::<i64>(&format!(
            "SELECT COUNT(*) FROM recent_events WHERE id >= '{}'::ulid AND id <= '{}'::ulid",
            oldest, newest
        ))
        .unwrap()
        .unwrap();
        assert_eq!(count, 10, "All events should be in range");

        // Get events after first one (exclusive)
        let count_after = Spi::get_one::<i64>(&format!(
            "SELECT COUNT(*) FROM recent_events WHERE id > '{}'::ulid",
            oldest
        ))
        .unwrap()
        .unwrap();
        assert_eq!(count_after, 9, "Should have 9 events after first");
    }

    /// Verify cursor-based pagination pattern
    #[pg_test]
    fn range_query_pagination() {
        let mut ulids = Vec::new();
        for _ in 0..10 {
            ulids.push(gen_ulid_mono());
        }

        Spi::run("CREATE TEMP TABLE paginated_items (id ulid PRIMARY KEY, item_num int)").unwrap();
        for (i, ulid) in ulids.iter().enumerate() {
            Spi::run(&format!(
                "INSERT INTO paginated_items VALUES ('{}'::ulid, {})",
                ulid, i
            ))
            .unwrap();
        }

        // Get item to use as cursor (3rd item)
        let cursor =
            Spi::get_one::<ULID>("SELECT id FROM paginated_items ORDER BY id LIMIT 1 OFFSET 2")
                .unwrap()
                .unwrap();

        // Get next page after cursor
        let next_item = Spi::get_one::<ULID>(&format!(
            "SELECT id FROM paginated_items WHERE id > '{}'::ulid ORDER BY id LIMIT 1",
            cursor
        ))
        .unwrap();

        assert!(next_item.is_some(), "Should have items after cursor");
        assert!(
            next_item.unwrap() > cursor,
            "Next item must be after cursor"
        );

        // Verify pagination continues beyond first page
        let first_page = Spi::get_one::<ULID>("SELECT id FROM paginated_items ORDER BY id LIMIT 1")
            .unwrap()
            .unwrap();

        let has_next_page = Spi::get_one::<bool>(&format!(
            "SELECT EXISTS(SELECT 1 FROM paginated_items WHERE id > '{}'::ulid)",
            first_page
        ))
        .unwrap()
        .unwrap();

        assert!(has_next_page, "Should have items on next page");
    }

    /// Verify range queries work with indexes
    #[pg_test]
    fn range_query_with_index() {
        let ulid1 = gen_ulid_mono();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ulid2 = gen_ulid_mono();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ulid3 = gen_ulid_mono();

        Spi::run("CREATE TEMP TABLE indexed_events (id ulid, data text)").unwrap();
        Spi::run("CREATE INDEX idx_id ON indexed_events(id)").unwrap();
        Spi::run(&format!(
            "INSERT INTO indexed_events VALUES 
            ('{}'::ulid, 'first'),
            ('{}'::ulid, 'second'),
            ('{}'::ulid, 'third')",
            ulid1, ulid2, ulid3
        ))
        .unwrap();

        // Range query should use index
        let data = Spi::get_one::<String>(&format!(
            "SELECT data FROM indexed_events 
             WHERE id >= '{}'::ulid AND id <= '{}'::ulid 
             ORDER BY id LIMIT 1",
            ulid1, ulid2
        ))
        .unwrap()
        .unwrap();

        assert_eq!(data, "first", "Should retrieve first item in range");
    }

    // ========================================================================
    // Aggregate Operation Tests
    // ========================================================================

    /// Verify DISTINCT works correctly
    #[pg_test]
    fn aggregate_distinct() {
        let ulid = gen_ulid();

        Spi::run("CREATE TEMP TABLE ulid_distinct (id ulid)").unwrap();
        Spi::run(&format!(
            "INSERT INTO ulid_distinct VALUES ('{}'::ulid), ('{}'::ulid), ('{}'::ulid)",
            ulid, ulid, ulid
        ))
        .unwrap();

        let count = Spi::get_one::<i64>("SELECT COUNT(DISTINCT id) FROM ulid_distinct")
            .unwrap()
            .unwrap();

        assert_eq!(count, 1, "Should count only unique ULIDs");
    }

    /// Verify GROUP BY works correctly
    #[pg_test]
    fn aggregate_group_by() {
        let ulid1 = gen_ulid();
        let ulid2 = gen_ulid();

        Spi::run("CREATE TEMP TABLE ulid_groups (id ulid, val int)").unwrap();
        Spi::run(&format!(
            "INSERT INTO ulid_groups VALUES ('{}'::ulid, 1), ('{}'::ulid, 2), ('{}'::ulid, 3)",
            ulid1, ulid1, ulid2
        ))
        .unwrap();

        let count = Spi::get_one::<i64>(&format!(
            "SELECT COUNT(*) FROM ulid_groups WHERE id = '{}'::ulid",
            ulid1
        ))
        .unwrap()
        .unwrap();

        assert_eq!(count, 2, "Should find 2 rows with first ULID");
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // Perform one-off initialization when the pg_test framework starts
    }

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // Return custom postgresql.conf settings for testing
        vec![]
    }
}
