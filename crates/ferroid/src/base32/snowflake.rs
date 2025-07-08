use crate::{Base32Error, Base32Ext, BeBytes, Error, Id, Result, Snowflake};

/// Extension trait for types that support Crockford Base32 encoding and
/// decoding.
///
/// This trait enables converting IDs (typically backed by primitive integers)
/// to and from fixed-length, lexicographically sortable Base32 strings using
/// the [Crockford Base32](https://www.crockford.com/base32.html) alphabet.
///
/// It relies on the [`BeBytes`] trait for bit-level access to the underlying
/// integer representation, and produces fixed-width ASCII-encoded output
/// suitable for ordered storage (e.g., in databases, file systems, or URLs).
///
/// # Features
///
/// - Zero-allocation encoding support
/// - Fixed-width, lexicographically sortable output
/// - ASCII-safe encoding using Crockford's Base32 alphabet
/// - Fallible decoding with strong validation
pub trait Base32SnowExt: Base32Ext + Snowflake
where
    Self::Ty: BeBytes,
{
    /// Encodes this ID into a [`String`] using Crockford Base32.
    ///
    /// The resulting string is guaranteed to be ASCII and lexicographically
    /// sortable.
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(feature = "base32")]
    /// {   
    ///     #[cfg(feature = "snowflake")]
    ///     {
    ///         use ferroid::{Base32SnowExt, SnowflakeTwitterId};
    ///         let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///         let encoded = id.encode();
    ///         assert_eq!(encoded, "23953MG16DJDJ");
    ///     }
    /// }
    /// ```
    fn encode(&self) -> String {
        Self::enc(&self)
    }
    /// Encodes this ID into the provided output buffer without heap allocation.
    ///
    /// This is the zero-allocation alternative to [`Base32SnowExt::encode`].
    /// The output buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes in
    /// length, which is guaranteed at compile time when using
    /// [`BeBytes::Base32Array`].
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(feature = "base32")]
    /// {   
    ///     #[cfg(feature = "snowflake")]
    ///     {
    ///         use ferroid::{Base32SnowExt, BeBytes, Id, SnowflakeTwitterId};
    ///         let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///
    ///         // Allocate a zeroed, stack-based buffer with the exact size required for encoding.
    ///         let mut buf = <<SnowflakeTwitterId as Id>::Ty as BeBytes>::Base32Array::default();
    ///         id.encode_to_buf(&mut buf);
    ///
    ///         // SAFETY: Crockford Base32 is guaranteed to produce valid ASCII
    ///         let encoded = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    ///         assert_eq!(encoded, "23953MG16DJDJ");
    ///     }
    /// }
    /// ```
    ///
    /// See also: [`Base32SnowExt::encode`] for an allocation-producing version.
    fn encode_to_buf(&self, buf: &mut <<Self as Id>::Ty as BeBytes>::Base32Array) {
        Self::enc_to_buf(&self, buf);
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:**  
    /// This method structurally decodes any 13-character Crockford base32
    /// string into a 64-bit integer, regardless of whether the input is a
    /// canonical Snowflake ID.  
    ///
    /// - If the input string is longer than the Snowflake's maximum
    ///   ("FZZZZZZZZZZZZ"), the excess bit is automatically truncated (i.e.,
    ///   the top 1 bit of the decoded value is discarded), so no overflow or
    ///   error occurs.
    /// - As a result, base32 strings that are technically invalid (i.e.,
    ///   lexicographically greater than the max Snowflake string) will still
    ///   successfully decode, with the truncated value.
    /// - **However**, if your ID type reserves bits (e.g., reserved or unused
    ///   bits in your layout), decoding a string with excess bits may set these
    ///   reserved bits to 1, causing `.is_valid()` to fail, and decode to
    ///   return an error.
    /// - For vanilla Snowflake IDs without reserved bits, decoding will always
    ///   succeed (truncating as needed), but for custom layouts, validation may
    ///   fail if reserved bits are set.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string:
    /// - is not the expected fixed length
    /// - contains invalid ASCII characters (i.e., not in the Crockford Base32
    ///   alphabet)
    /// - sets reserved bits that make the decoded value invalid for this ID
    ///   type
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(feature = "base32")] {   
    ///     #[cfg(feature = "snowflake")]
    ///     {
    ///         use ferroid::{Base32SnowExt, Snowflake, SnowflakeTwitterId, Error, Base32Error, Id};
    ///         // --- Success case ---
    ///         let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///         let encoded = id.encode();
    ///         let decoded = SnowflakeTwitterId::decode(&encoded).unwrap();
    ///         assert_eq!(decoded, id);
    ///
    ///         // --- Failure/overflow case ---
    ///         // Construct a string which decodes to max value (overflow for some ID types).
    ///         match SnowflakeTwitterId::decode("FZZZZZZZZZZZZ") {
    ///             Ok(_) => panic!("Should not succeed!"),
    ///             Err(Error::Base32Error(Base32Error::DecodeOverflow(bytes))) => {
    ///                 // Reconstruct the primitive value from bytes
    ///                 let prim = u64::from_be_bytes(bytes.try_into().unwrap());
    ///                 // Recover as a raw ID
    ///                 let invalid = SnowflakeTwitterId::from_raw(prim);
    ///                 // Optionally, normalize and continue:
    ///                 let valid = invalid.into_valid();
    ///                 // `valid` is now zeroed out any reserved bits.
    ///             }
    ///             Err(e) => panic!("Unexpected error: {e:?}"),
    ///         }
    ///     }
    /// }
    /// ```
    fn decode(s: &str) -> Result<Self> {
        let decoded = Self::dec(s)?;
        if !decoded.is_valid() {
            return Err(Error::Base32Error(Base32Error::DecodeOverflow(
                decoded.to_raw().to_be_bytes().as_ref().to_vec(),
            )));
        }
        Ok(decoded)
    }
}

impl<ID> Base32SnowExt for ID
where
    ID: Snowflake,
    ID::Ty: BeBytes,
{
}

#[cfg(test)]
mod tests {
    use crate::{
        Base32Error, Base32SnowExt, Error, Snowflake, SnowflakeDiscordId, SnowflakeInstagramId,
        SnowflakeMastodonId, SnowflakeTwitterId,
    };

    #[test]
    fn twitter_max() {
        let id = SnowflakeTwitterId::from_components(
            SnowflakeTwitterId::max_timestamp(),
            SnowflakeTwitterId::max_machine_id(),
            SnowflakeTwitterId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeTwitterId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeTwitterId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeTwitterId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "7ZZZZZZZZZZZZ");
        let decoded = SnowflakeTwitterId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeTwitterId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeTwitterId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeTwitterId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn twitter_zero() {
        let id = SnowflakeTwitterId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeTwitterId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn discord_max() {
        let id = SnowflakeDiscordId::from_components(
            SnowflakeDiscordId::max_timestamp(),
            SnowflakeDiscordId::max_machine_id(),
            SnowflakeDiscordId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeDiscordId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeDiscordId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeDiscordId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "FZZZZZZZZZZZZ");
        let decoded = SnowflakeDiscordId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeDiscordId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeDiscordId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeDiscordId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn discord_zero() {
        let id = SnowflakeDiscordId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeDiscordId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn instagram_max() {
        let id = SnowflakeInstagramId::from_components(
            SnowflakeInstagramId::max_timestamp(),
            SnowflakeInstagramId::max_machine_id(),
            SnowflakeInstagramId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeInstagramId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeInstagramId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeInstagramId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "FZZZZZZZZZZZZ");
        let decoded = SnowflakeInstagramId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeInstagramId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeInstagramId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeInstagramId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn instagram_zero() {
        let id = SnowflakeInstagramId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeInstagramId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn mastodon_max() {
        let id = SnowflakeMastodonId::from_components(
            SnowflakeMastodonId::max_timestamp(),
            SnowflakeMastodonId::max_machine_id(),
            SnowflakeMastodonId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeMastodonId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeMastodonId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeMastodonId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "FZZZZZZZZZZZZ");
        let decoded = SnowflakeMastodonId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeMastodonId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeMastodonId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeMastodonId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn mastodon_zero() {
        let id = SnowflakeMastodonId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeMastodonId::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn decode_invalid_character_fails() {
        // Base32 Crockford disallows symbols like `@`
        let invalid = "012345678901@";
        let result = SnowflakeTwitterId::decode(invalid);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidAscii(64)))
        ));
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_short = "012345678901";
        let result = SnowflakeTwitterId::decode(too_short);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen(12)))
        ));

        // Longer than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_long = "01234567890123";
        let result = SnowflakeTwitterId::decode(too_long);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen(14)))
        ));
    }
}
