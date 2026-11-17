mod buffers;

use ferroid::{
    base32::Base32Ext,
    generator::thread_local::cached::Ulid,
    id::{BeBytes, Id, ULID as InnerULID},
};
use pgrx::{
    PgMemoryContexts,
    callconv::{ArgAbi, BoxRet},
    datum::{FromDatum, IntoDatum},
    pg_sys::{self, AsPgCStr, errcodes::PgSqlErrorCode},
    pgrx_sql_entity_graph::metadata::{
        ArgumentError, ReturnsError, ReturnsRef, SqlMappingRef, SqlTranslatable, TypeOrigin,
    },
    prelude::*,
    rust_regtypein,
};

use crate::buffers::{UlidBytea, UlidCString, UlidText};

pgrx::pg_module_magic!();

// ============================================================================
// ULID
// ============================================================================

/// Big-endian byte array for the backing integer of `InnerULID`.
///
/// This is the `BeBytes::ByteArray` for `<InnerULID as Id>::Ty`, so it is a
/// fixed-size `[u8; SIZE]` (currently `[u8; 16]` for ULIDs).
pub type Bytes = <<InnerULID as Id>::Ty as BeBytes>::ByteArray;

/// Size of the backing integer for `InnerULID`, in bytes.
///
/// This is identical to `core::mem::size_of::<Bytes>()`.
pub const SIZE: usize = <<InnerULID as Id>::Ty as BeBytes>::SIZE;

/// A PostgreSQL ULID type backed by `ferroid`.
///
/// Represents a 128-bit, lexicographically sortable identifier with
/// timestamp-first ordering.
///
/// Storage characteristics:
/// - Fixed width: 16 bytes
/// - Representation: big-endian binary ULID
/// - Passed by reference (not by value)
/// - Optimized for B-tree indexing
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, PostgresHash, PostgresEq, PostgresOrd,
)]
#[repr(transparent)]
pub struct ULID {
    pub(crate) bytes: Bytes,
}

impl ULID {
    pub const fn from_bytes(bytes: Bytes) -> Self {
        Self { bytes }
    }

    pub const fn as_bytes(&self) -> &Bytes {
        &self.bytes
    }

    pub(crate) fn timestamp(&self) -> i64 {
        // The ULID timestamp is 48 bits, which always fits in i64 (63 bits) so
        // this cast is guaranteed not to overflow.
        InnerULID::from(self).timestamp() as i64
    }
}

impl From<InnerULID> for ULID {
    fn from(ulid: InnerULID) -> Self {
        Self::from_bytes(ulid.to_raw().to_be_bytes())
    }
}

impl From<&ULID> for InnerULID {
    fn from(p: &ULID) -> Self {
        Self::from_raw(<<Self as Id>::Ty>::from_be_bytes(p.bytes))
    }
}

impl From<ULID> for InnerULID {
    fn from(p: ULID) -> Self {
        Self::from(&p)
    }
}

impl core::fmt::Display for ULID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        InnerULID::from(self).encode().fmt(f)
    }
}

// ============================================================================
// DATUM + CALLCONV GLUE
// ============================================================================

impl FromDatum for ULID {
    /// # Safety
    ///
    /// - When is_null is false, datum must point to SIZE initialized bytes of a
    ///   SQL ulid, readable for this call.
    /// - The bytes are copied with no alignment requirement or retained borrow.
    unsafe fn from_polymorphic_datum(
        datum: pg_sys::Datum,
        is_null: bool,
        _typoid: pg_sys::Oid,
    ) -> Option<Self> {
        if is_null {
            return None;
        }

        let ptr = datum.cast_mut_ptr::<Bytes>() as *const Bytes;
        // SAFETY:
        // - The caller supplies a non-NULL, pass-by-reference ulid datum with
        //   SIZE initialized bytes.
        // - Every byte pattern is valid for Bytes.
        // - read_unaligned makes no additional alignment assumption.
        // - The read copies the value without retaining a PostgreSQL reference.
        let bytes = unsafe { ptr.read_unaligned() };
        Some(ULID::from_bytes(bytes))
    }
}

impl IntoDatum for ULID {
    fn into_datum(self) -> Option<pg_sys::Datum> {
        // SAFETY:
        // - This conversion runs inside a PostgreSQL call with an active
        //   CurrentMemoryContext.
        // - palloc_struct allocates fresh, writable storage sized and aligned
        //   for Bytes, or raises an error.
        // - PostgreSQL manages the allocation until its owning memory context
        //   is reset or deleted.
        let raw = unsafe { PgMemoryContexts::CurrentMemoryContext.palloc_struct::<Bytes>() };

        // SAFETY:
        // - raw is the fresh allocation above, with exclusive access to space
        //   for one Bytes value.
        // - The assignment initializes the entire byte array before the pointer
        //   is returned to PostgreSQL.
        unsafe {
            *raw = self.bytes;
        }

        Some(raw.into())
    }

    fn type_oid() -> pg_sys::Oid {
        rust_regtypein::<Self>()
    }
}

// SAFETY:
// - SQL ulid is passed by reference as SIZE big-endian bytes, matching
//   FromDatum's representation.
// - Unboxing produces an owned copy and retains no argument-storage references.
unsafe impl<'fcx> ArgAbi<'fcx> for ULID
where
    Self: 'fcx,
{
    /// # Safety
    ///
    /// - arg must be a non-NULL SQL ulid argument with SIZE readable bytes.
    /// - The call must satisfy ArgAbi's contract; unboxing returns an owned copy.
    unsafe fn unbox_arg_unchecked(arg: ::pgrx::callconv::Arg<'_, 'fcx>) -> Self {
        // SAFETY:
        // - ArgAbi's caller guarantees the matching logical type and a non-NULL
        //   argument.
        // - FromDatum copies the initialized bytes into an owned ULID without
        //   extending the argument's lifetime.
        unsafe {
            arg.unbox_arg_using_from_datum()
                .unwrap_or_else(|| pgrx::error!("ULID argument must not be NULL"))
        }
    }
}

// SAFETY:
// - IntoDatum creates the initialized, PostgreSQL-owned representation declared
//   by SqlTranslatable and CREATE TYPE ulid.
// - Boxing returns that datum as the result of the same PostgreSQL call.
unsafe impl BoxRet for ULID {
    /// # Safety
    ///
    /// - fcinfo must be the active PostgreSQL call returning SQL ulid, with a
    ///   live current memory context.
    /// - The returned datum must be returned as that call's result, as required
    ///   by BoxRet; PostgreSQL owns its initialized allocation.
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        // SAFETY:
        // - fcinfo satisfies BoxRet's call contract.
        // - IntoDatum returns a fully initialized ulid allocation in the current
        //   memory context.
        // - The datum is returned directly; no Rust borrow escapes.
        unsafe { fcinfo.return_optional_datum(self.into_datum()) }
    }
}

// SAFETY:
// - This extension declares SQL ulid with the fixed-size, pass-by-reference byte
//   representation used by FromDatum, IntoDatum, ArgAbi, and BoxRet.
// - ThisExtension links the mapping to its CREATE TYPE declaration.
unsafe impl SqlTranslatable for ULID {
    const TYPE_IDENT: &'static str = pgrx::pgrx_resolved_type!(ULID);
    const TYPE_ORIGIN: TypeOrigin = TypeOrigin::ThisExtension;
    const ARGUMENT_SQL: Result<SqlMappingRef, ArgumentError> = Ok(SqlMappingRef::literal("ulid"));
    const RETURN_SQL: Result<ReturnsRef, ReturnsError> =
        Ok(ReturnsRef::One(SqlMappingRef::literal("ulid")));
}

// ============================================================================
// TEXT I/O (Base32 encoding/decoding)
// ============================================================================
#[pg_extern(immutable, parallel_safe, strict, requires = ["shell_type"])]
fn ulid_in(input: &core::ffi::CStr) -> ULID {
    InnerULID::decode(input.to_bytes())
        .map(ULID::from)
        .unwrap_or_else(|e| {
            pgrx::ereport!(
                ERROR,
                PgSqlErrorCode::ERRCODE_INVALID_TEXT_REPRESENTATION,
                format!("invalid ULID: {e}")
            );
        })
}

#[pg_extern(immutable, parallel_safe, strict, requires = ["shell_type"])]
fn ulid_out(ulid: ULID) -> UlidCString {
    let encoded = InnerULID::from(ulid).encode();
    // SAFETY:
    // - as_pg_cstr allocates and initializes a NUL-terminated copy in
    //   CurrentMemoryContext, or raises an error.
    // - Base32 has no interior NUL.
    // - PgBox::from_pg wraps the PostgreSQL-owned pointer without borrowing the
    //   local encoded buffer or taking over memory-context cleanup.
    UlidCString(unsafe { PgBox::from_pg(encoded.as_str().as_pg_cstr()) })
}

// ============================================================================
// ULID GENERATION
// ============================================================================

/// Generates a non-monotonic ULID using the shared wall-clock cache.
///
/// The cache refreshes roughly once per millisecond.
#[pg_extern(strict, parallel_safe)]
fn gen_ulid() -> ULID {
    Ulid::new_ulid().into()
}

/// Generates a new monotonic ULID.
///
/// IDs are strictly increasing within each backend. If wall time moves
/// backwards, generation waits for it to catch up and remains cancellable.
#[pg_extern(strict, parallel_safe)]
fn gen_ulid_mono() -> ULID {
    Ulid::new_ulid_mono(|_| {
        pgrx::check_for_interrupts();
        std::thread::yield_now();
    })
    .into()
}

// ============================================================================
// CASTING SUPPORT
// ============================================================================

// PostgreSQL epoch: 2000-01-01 00:00:00 UTC Unix epoch: 1970-01-01 00:00:00 UTC
// Difference: 946,684,800 seconds = 946,684,800,000,000 microseconds.
const PG_EPOCH_OFFSET_MICROS: i64 = 946_684_800_000_000;

/// Casts ULID to timestamptz (requires explicit cast).
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_timestamptz(ulid: ULID) -> TimestampWithTimeZone {
    // A 48-bit millisecond timestamp fits in i64 after scaling and epoch adjustment.
    let pg_micros = ulid.timestamp() * 1_000 - PG_EPOCH_OFFSET_MICROS;

    TimestampWithTimeZone::try_from(pg_micros).unwrap_or_else(|e| {
        pgrx::ereport!(
            ERROR,
            PgSqlErrorCode::ERRCODE_DATETIME_FIELD_OVERFLOW,
            format!("timestamp out of range: {e}")
        );
    })
}

/// Casts timestamptz to ULID (requires explicit cast).
///
/// Truncates to millisecond precision and sets the random component to zero.
/// Rejects non-finite timestamps and values outside the ULID timestamp range.
#[pg_cast(immutable, parallel_safe, strict)]
fn timestamptz_to_ulid(ts: TimestampWithTimeZone) -> ULID {
    const MIN_PG_MICROS: i64 = -PG_EPOCH_OFFSET_MICROS;
    const END_PG_MICROS: i64 =
        (1_i64 << InnerULID::TIMESTAMP_BITS) * 1_000 - PG_EPOCH_OFFSET_MICROS;

    let pg_micros: i64 = ts.into();
    if !(MIN_PG_MICROS..END_PG_MICROS).contains(&pg_micros) {
        pgrx::ereport!(
            ERROR,
            PgSqlErrorCode::ERRCODE_DATETIME_FIELD_OVERFLOW,
            "timestamp outside ULID range"
        );
    }

    // The validated range makes the addition safe and the result nonnegative.
    #[allow(clippy::cast_sign_loss)]
    let ms = ((pg_micros + PG_EPOCH_OFFSET_MICROS) / 1_000) as u128;
    InnerULID::from_components(ms, 0).into()
}

/// Casts text to ULID (requires explicit cast).
#[pg_cast(immutable, parallel_safe, strict)]
fn text_to_ulid(text: &str) -> ULID {
    InnerULID::decode(text).map(ULID::from).unwrap_or_else(|e| {
        pgrx::ereport!(
            ERROR,
            PgSqlErrorCode::ERRCODE_INVALID_TEXT_REPRESENTATION,
            format!("invalid ULID text: {e}")
        );
    })
}

/// Casts ULID to text (requires explicit cast).
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_text(ulid: ULID) -> UlidText {
    let encoded = InnerULID::from(ulid).encode();
    UlidText(pgrx::varlena::rust_str_to_text_p(encoded.as_str()))
}

/// Casts bytea to ULID (requires explicit cast).
#[pg_cast(immutable, parallel_safe, strict)]
fn bytea_to_ulid(bytes: &[u8]) -> ULID {
    let arr: Bytes = bytes.try_into().unwrap_or_else(|_| {
        pgrx::ereport!(
            ERROR,
            PgSqlErrorCode::ERRCODE_INVALID_BINARY_REPRESENTATION,
            format!(
                "invalid bytea length for ulid: expected {} bytes, got {}",
                SIZE,
                bytes.len()
            )
        );
    });
    ULID::from_bytes(arr)
}

/// Casts ULID to bytea (requires explicit cast).
#[pg_cast(immutable, parallel_safe, strict)]
fn ulid_to_bytea(ulid: ULID) -> UlidBytea {
    UlidBytea(pgrx::varlena::rust_byte_slice_to_bytea(ulid.as_bytes()))
}

// ============================================================================
// BINARY PROTOCOL
// ============================================================================

/// Receives a ULID from the PostgreSQL binary protocol.
#[pg_extern(immutable, parallel_safe, strict, requires = ["shell_type"])]
fn ulid_recv(mut internal: pgrx::Internal) -> ULID {
    // SAFETY:
    // - PostgreSQL's RECEIVE contract supplies a live StringInfoData pointer
    //   through Internal.
    // - The call has exclusive access to the message cursor.
    // - The registered ulid receive function uses that exact ABI.
    // - pq_getmsgbytes checks the remaining length before SIZE bytes are copied
    //   with read_unaligned; no reference to message storage escapes.
    unsafe {
        // The RECEIVE argument points to StringInfoData; it remains live while
        // get_mut borrows it and the message-reading functions advance it.
        let buf = internal
            .get_mut::<pg_sys::StringInfoData>()
            .unwrap_or_else(|| pgrx::error!("ulid_recv: NULL internal buffer"));

        // SIZE is 16 and fits in i32. PostgreSQL checks that all requested bytes
        // remain before returning their pointer, raising ERROR if truncated.
        let src = pg_sys::pq_getmsgbytes(buf, SIZE as i32) as *const Bytes;
        // The returned pointer spans SIZE initialized bytes. Bytes accepts all
        // bit patterns, and read_unaligned copies them without retaining a
        // pointer into the input message or requiring stronger alignment.
        let out = src.read_unaligned();
        // buf is still live with its updated cursor. Reject any trailing bytes
        // so the complete binary representation is exactly SIZE bytes.
        pg_sys::pq_getmsgend(buf);
        ULID::from_bytes(out)
    }
}

/// Sends a ULID using the PostgreSQL binary protocol.
#[pg_extern(immutable, parallel_safe, strict, requires = ["shell_type"])]
fn ulid_send(ulid: ULID) -> UlidBytea {
    ulid_to_bytea(ulid)
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Checks if a string is a valid ULID.
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
    SEND = ulid_send,
    RECEIVE = ulid_recv,
    INTERNALLENGTH = 16,
    ALIGNMENT = char,
    STORAGE = plain,
    PASSEDBYVALUE = false
);
"#,
    name = "concrete_type",
    creates = [Type(ULID)],
    requires = ["shell_type", ulid_in, ulid_out, ulid_send, ulid_recv]
);
extension_sql!(
    r#"
COMMENT ON TYPE ulid IS 'Universally Unique Lexicographically Sortable Identifier - 128-bit identifier with timestamp ordering';
COMMENT ON FUNCTION gen_ulid() IS 'Generate a new random ULID';
COMMENT ON FUNCTION gen_ulid_mono() IS 'Generate a monotonic ULID (strictly increasing per backend)';
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
    // pgrx requires the schema declaration to remain an inline module.
    include!("tests.rs");
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
