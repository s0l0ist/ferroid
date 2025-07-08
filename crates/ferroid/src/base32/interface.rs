use crate::{Base32Error, BeBytes, Error, Id, Result};

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
pub trait Base32Ext: Id
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
    ///     use ferroid::Base32Ext;
    ///
    ///
    ///     #[cfg(feature = "ulid")]
    ///     {
    ///         use ferroid::ULID;
    ///         let id = ULID::from_raw(2_424_242_424_242_424_242);
    ///         let encoded = id.encode();
    ///         assert_eq!(encoded, "000000000000023953MG16DJDJ");
    ///     }
    ///     #[cfg(feature = "snowflake")]
    ///     {
    ///         use ferroid::SnowflakeTwitterId;
    ///         let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///         let encoded = id.encode();
    ///         assert_eq!(encoded, "23953MG16DJDJ");
    ///     }
    /// }
    /// ```
    fn encode(&self) -> String {
        let mut buf = <Self::Ty as BeBytes>::Base32Array::default();
        self.encode_to_buf(&mut buf);

        // SAFETY: Crockford Base32 output is always valid ASCII
        unsafe { String::from_utf8_unchecked(buf.as_ref().to_vec()) }
    }
    /// Encodes this ID into the provided output buffer without heap allocation.
    ///
    /// This is the zero-allocation alternative to [`Base32Ext::encode`]. The
    /// output buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes in length,
    /// which is guaranteed at compile time when using [`BeBytes::Base32Array`].
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(feature = "base32")]
    /// {   
    ///     use ferroid::Base32Ext;
    ///
    ///
    ///     #[cfg(feature = "ulid")]
    ///     {
    ///         use ferroid::{BeBytes, Id, ULID};
    ///         let id = ULID::from_raw(2_424_242_424_242_424_242);
    ///
    ///         // Allocate a zeroed, stack-based buffer with the exact size required for encoding.
    ///         let mut buf = <<ULID as Id>::Ty as BeBytes>::Base32Array::default();
    ///         id.encode_to_buf(&mut buf);
    ///
    ///         // SAFETY: Crockford Base32 is guaranteed to produce valid ASCII
    ///         let encoded = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    ///         assert_eq!(encoded, "000000000000023953MG16DJDJ");
    ///     }
    /// }
    /// ```
    ///
    /// See also: [`Base32Ext::encode`] for an allocation-producing version.
    fn encode_to_buf(&self, buf: &mut <<Self as Id>::Ty as BeBytes>::Base32Array) {
        super::encode_base32(self.to_raw().to_be_bytes().as_ref(), buf.as_mut());
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:** This method performs a structural decode of the Base32
    /// string into the raw underlying integer. It does **not** validate whether
    /// the decoded value satisfies semantic invariants of the ID format (e.g.,
    /// reserved bits).
    ///
    /// If your ID type includes reserved bits, you should explicitly validate
    /// the result using `.is_valid()` or normalize it using `.into_valid()`.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string:
    /// - is not the expected fixed length
    /// - contains invalid ASCII characters (i.e., not in the Crockford Base32
    ///   alphabet)
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(feature = "base32")]
    /// {   
    ///     use ferroid::Base32Ext;
    /// }
    /// ```
    fn decode(s: &str) -> Result<Self> {
        if s.len() != Self::Ty::BASE32_SIZE {
            return Err(Error::Base32Error(Base32Error::DecodeInvalidLen(s.len())));
        }
        let raw = super::decode_base32(s)?;
        Ok(Self::from_raw(raw))
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}

#[cfg(all(test, feature = "snowflake"))]
mod snowflake_tests {
    use crate::{
        Base32Error, Base32Ext, Error, Snowflake, SnowflakeDiscordId, SnowflakeInstagramId,
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

#[cfg(all(test, feature = "ulid"))]
mod ulid_tests {
    use crate::{Base32Ext, ULID, Ulid};

    #[test]
    fn ulid_max() {
        let id = ULID::from_components(ULID::max_timestamp(), ULID::max_random());
        assert_eq!(id.timestamp(), ULID::max_timestamp());
        assert_eq!(id.random(), ULID::max_random());

        let encoded = id.encode();
        assert_eq!(encoded, "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
        let decoded = ULID::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), ULID::max_timestamp());
        assert_eq!(decoded.random(), ULID::max_random());
        assert_eq!(id, decoded);
    }

    #[test]
    fn ulid_known() {
        let id = ULID::from_components(1469922850259, 1012768647078601740696923);
        assert_eq!(id.timestamp(), 1469922850259);
        assert_eq!(id.random(), 1012768647078601740696923);

        let encoded = id.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        let decoded = ULID::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 1469922850259);
        assert_eq!(decoded.random(), 1012768647078601740696923);
        assert_eq!(id, decoded);

        let id = ULID::from_components(1611559180765, 885339478614498720052741);
        assert_eq!(id.timestamp(), 1611559180765);
        assert_eq!(id.random(), 885339478614498720052741);

        let encoded = id.encode();
        assert_eq!(encoded, "01EWW6K6EXQDX5JV0E9CAHPXG5");
        let decoded = ULID::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 1611559180765);
        assert_eq!(decoded.random(), 885339478614498720052741);
        assert_eq!(id, decoded);
    }

    #[test]
    fn ulid_zero() {
        let id = ULID::from_components(0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.random(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "00000000000000000000000000");
        let decoded = ULID::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.random(), 0);
        assert_eq!(id, decoded);
    }
}
