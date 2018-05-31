#![allow(dead_code)]

use std::time::Instant;

use base64;
use chrono::{DateTime, Duration, Utc};
use serde::Serializer;
use url::Url;

base64_serde_type!(pub StandardBase64, base64::STANDARD);

pub fn serialize_origin<S>(url: &Option<Url>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(url) = url {
        let string = url.origin().ascii_serialization();
        serializer.serialize_some(&string)
    } else {
        serializer.serialize_none()
    }
}

lazy_static! {
    static ref REF_TIME: (Instant, DateTime<Utc>) = (Instant::now(), Utc::now());
}

pub fn best_effort_instant_to_datetime(instant: Instant) -> DateTime<Utc> {
    Duration::from_std(instant.duration_since(REF_TIME.0))
        .ok()
        .and_then(|x| REF_TIME.1.checked_add_signed(x))
        .unwrap_or_else(Utc::now)
}

pub fn best_effort_datetime_to_instant(dt: DateTime<Utc>) -> Instant {
    if let Some(rv) = dt.signed_duration_since(REF_TIME.1)
        .to_std()
        .ok()
        .map(|x| REF_TIME.0 + x)
    {
        return rv;
    }
    if let Some(rv) = REF_TIME
        .1
        .signed_duration_since(dt)
        .to_std()
        .ok()
        .map(|x| REF_TIME.0 - x)
    {
        return rv;
    }

    Instant::now()
}

pub mod instant_serde {
    use super::*;
    use serde::{de, ser};
    use serde::{Deserialize, Serialize};

    pub fn deserialize<'de, D>(d: D) -> Result<Instant, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Ok(best_effort_datetime_to_instant(
            DateTime::<Utc>::deserialize(d)?,
        ))
    }

    pub fn serialize<S>(i: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        best_effort_instant_to_datetime(i.clone()).serialize(serializer)
    }
}
