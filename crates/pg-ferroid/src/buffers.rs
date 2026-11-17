//! PostgreSQL-allocated return buffers for ULID text and binary output.

use pgrx::{PgBox, callconv::BoxRet, pg_sys};

/// Returns a PostgreSQL-owned C string as SQL `cstring` without another copy.
pub(crate) struct UlidCString(pub(crate) PgBox<core::ffi::c_char>);

pgrx::impl_sql_translatable!(UlidCString, "cstring");

// SAFETY:
// - The SQL cstring mapping matches the initialized, NUL-terminated PgBox buffer.
// - Boxing delegates to PgBox and preserves PostgreSQL's allocation ownership.
unsafe impl BoxRet for UlidCString {
    /// # Safety
    ///
    /// - fcinfo must be the active PostgreSQL call returning SQL cstring.
    /// - The returned datum must be returned as that call's result; delegation
    ///   preserves its initialized buffer and PostgreSQL ownership.
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        // SAFETY:
        // - The caller supplies a valid return context for this SQL type.
        // - Delegating with the same fcinfo returns the existing allocation.
        // - Consuming the PgBox neither frees the buffer nor retains a borrow.
        unsafe { self.0.box_into(fcinfo) }
    }
}

/// Returns a PostgreSQL-owned buffer as SQL `text` without another copy.
pub(crate) struct UlidText(pub(crate) PgBox<pg_sys::text>);

pgrx::impl_sql_translatable!(UlidText, "text");

// SAFETY:
// - The SQL text mapping matches the initialized varlena PgBox buffer.
// - Boxing delegates to PgBox and preserves PostgreSQL's allocation ownership.
unsafe impl BoxRet for UlidText {
    /// # Safety
    ///
    /// - fcinfo must be the active PostgreSQL call returning SQL text.
    /// - The returned datum must be returned as that call's result; delegation
    ///   preserves its initialized buffer and PostgreSQL ownership.
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        // SAFETY:
        // - fcinfo meets BoxRet's call contract for SQL text.
        // - PgBox's implementation returns the initialized datum directly.
        // - Consuming the wrapper neither frees the buffer nor extends a borrow.
        unsafe { self.0.box_into(fcinfo) }
    }
}

/// Returns a PostgreSQL-owned buffer as SQL `bytea` without another copy.
pub(crate) struct UlidBytea(pub(crate) PgBox<pg_sys::bytea>);

pgrx::impl_sql_translatable!(UlidBytea, "bytea");

// SAFETY:
// - The SQL bytea mapping matches the initialized varlena PgBox buffer.
// - Boxing delegates to PgBox and preserves PostgreSQL's allocation ownership.
unsafe impl BoxRet for UlidBytea {
    /// # Safety
    ///
    /// - fcinfo must be the active PostgreSQL call returning SQL bytea.
    /// - The returned datum must be returned as that call's result; delegation
    ///   preserves its initialized buffer and PostgreSQL ownership.
    unsafe fn box_into<'fcx>(
        self,
        fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
    ) -> pgrx::datum::Datum<'fcx> {
        // SAFETY:
        // - fcinfo meets BoxRet's call contract for SQL bytea.
        // - Delegation uses the same return context and initialized datum.
        // - PostgreSQL owns the allocation after the PgBox is consumed.
        unsafe { self.0.box_into(fcinfo) }
    }
}
