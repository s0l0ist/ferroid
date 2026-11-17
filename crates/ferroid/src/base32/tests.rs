#[cfg(feature = "ulid")]
mod ulid {
    #[cfg(test)]
    mod alloc_test {
        use alloc::string::ToString;

        use crate::{base32::Base32Ext, id::ULID};

        #[test]
        fn ulid_display() {
            let ulid = ULID::decode("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
            assert_eq!(alloc::format!("{ulid}"), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
            assert_eq!(ulid.to_string(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        }
    }

    #[cfg(test)]
    mod test {
        use crate::{
            base32::{Base32Ext, Error},
            id::ULID,
        };

        #[test]
        fn ulid_try_from() {
            let ulid = ULID::try_from("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
            let encoded = ulid.encode();
            assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        }

        #[test]
        fn ulid_from_str() {
            use core::str::FromStr;
            let ulid = ULID::from_str("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
            let encoded = ulid.encode();
            assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        }

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
            let id = ULID::from_components(1_469_922_850_259, 1_012_768_647_078_601_740_696_923);
            assert_eq!(id.timestamp(), 1_469_922_850_259);
            assert_eq!(id.random(), 1_012_768_647_078_601_740_696_923);

            let encoded = id.encode();
            assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
            let decoded = ULID::decode(encoded).unwrap();

            assert_eq!(decoded.timestamp(), 1_469_922_850_259);
            assert_eq!(decoded.random(), 1_012_768_647_078_601_740_696_923);
            assert_eq!(id, decoded);

            let id = ULID::from_components(1_611_559_180_765, 885_339_478_614_498_720_052_741);
            assert_eq!(id.timestamp(), 1_611_559_180_765);
            assert_eq!(id.random(), 885_339_478_614_498_720_052_741);

            let encoded = id.encode();
            assert_eq!(encoded, "01EWW6K6EXQDX5JV0E9CAHPXG5");
            let decoded = ULID::decode(encoded).unwrap();

            assert_eq!(decoded.timestamp(), 1_611_559_180_765);
            assert_eq!(decoded.random(), 885_339_478_614_498_720_052_741);
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

        #[test]
        fn decode_invalid_character_fails() {
            // Base32 Crockford disallows symbols like `@`
            let invalid = "000000000000@0000000000000";
            let res = ULID::decode(invalid);
            assert_eq!(
                res.unwrap_err(),
                Error::DecodeInvalidAscii {
                    byte: b'@',
                    index: 12,
                }
            );
        }

        #[test]
        fn decode_invalid_length_fails() {
            // Shorter than 26-byte base32 for u128
            let too_short = "0123456789012345678901234";
            let res = ULID::decode(too_short);
            assert_eq!(res.unwrap_err(), Error::DecodeInvalidLen { len: 25 });

            // Longer than 26-byte base32 for u128
            let too_long = "012345678901234567890123456";
            let res = ULID::decode(too_long);

            assert_eq!(res.unwrap_err(), Error::DecodeInvalidLen { len: 27 });
        }
    }
}

#[cfg(feature = "snowflake")]
mod snowflake {
    #[cfg(test)]
    mod alloc_test {
        use alloc::string::ToString;

        use crate::{
            base32::Base32Ext,
            id::{
                SnowflakeDiscordId, SnowflakeInstagramId, SnowflakeMastodonId, SnowflakeTwitterId,
            },
        };

        #[test]
        fn twitter_display() {
            let id = SnowflakeTwitterId::decode("01ARZ3NDEKTSV").unwrap();
            assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
            assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
        }
        #[test]
        fn instagram_display() {
            let id = SnowflakeInstagramId::decode("01ARZ3NDEKTSV").unwrap();
            assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
            assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
        }
        #[test]
        fn mastodon_display() {
            let id = SnowflakeMastodonId::decode("01ARZ3NDEKTSV").unwrap();
            assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
            assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
        }
        #[test]
        fn discord_display() {
            let id = SnowflakeDiscordId::decode("01ARZ3NDEKTSV").unwrap();
            assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
            assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
        }
    }

    #[cfg(test)]
    mod test {
        use crate::{
            base32::{Base32Ext, Error},
            id::{
                SnowflakeDiscordId, SnowflakeInstagramId, SnowflakeMastodonId, SnowflakeTwitterId,
            },
        };

        #[test]
        fn snow_try_from() {
            // Don't need to test all IDs
            let id = SnowflakeTwitterId::try_from("01ARZ3NDEKTSV").unwrap();
            let encoded = id.encode();
            assert_eq!(encoded, "01ARZ3NDEKTSV");
        }

        #[test]
        fn snow_from_str() {
            // Don't need to test all IDs
            use core::str::FromStr;
            let id = SnowflakeTwitterId::from_str("01ARZ3NDEKTSV").unwrap();
            let encoded = id.encode();
            assert_eq!(encoded, "01ARZ3NDEKTSV");
        }

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
            let decoded = SnowflakeTwitterId::decode(encoded).unwrap();

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
            let decoded = SnowflakeTwitterId::decode(encoded).unwrap();

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
            let decoded = SnowflakeDiscordId::decode(encoded).unwrap();

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
            let decoded = SnowflakeDiscordId::decode(encoded).unwrap();

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
            let decoded = SnowflakeInstagramId::decode(encoded).unwrap();

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
            let decoded = SnowflakeInstagramId::decode(encoded).unwrap();

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
            let decoded = SnowflakeMastodonId::decode(encoded).unwrap();

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
            let decoded = SnowflakeMastodonId::decode(encoded).unwrap();

            assert_eq!(decoded.timestamp(), 0);
            assert_eq!(decoded.machine_id(), 0);
            assert_eq!(decoded.sequence(), 0);
            assert_eq!(id, decoded);
        }

        #[test]
        fn decode_invalid_character_fails() {
            // Base32 Crockford disallows symbols like `@`
            let invalid = "000000@000000";
            let res = SnowflakeTwitterId::decode(invalid);
            assert_eq!(
                res.unwrap_err(),
                Error::DecodeInvalidAscii {
                    byte: b'@',
                    index: 6,
                }
            );
        }

        #[test]
        fn decode_invalid_length_fails() {
            // Shorter than 13-byte base32 for u64
            let too_short = "012345678901";
            let res = SnowflakeTwitterId::decode(too_short);
            assert_eq!(res.unwrap_err(), Error::DecodeInvalidLen { len: 12 });

            // Longer than 13-byte base32 for u64
            let too_long = "01234567890123";
            let res = SnowflakeTwitterId::decode(too_long);

            assert_eq!(res.unwrap_err(), Error::DecodeInvalidLen { len: 14 });
        }
    }
}
