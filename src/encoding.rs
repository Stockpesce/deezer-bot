use base64::{prelude::BASE64_STANDARD_NO_PAD, Engine};
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

/// module for encoding very little things

#[derive(Error, Debug)]
pub enum EncoderError {
    #[error(transparent)]
    MessagePackEncoding(#[from] rmp_serde::encode::Error),

    #[error("error decoding base64: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error(transparent)]
    MessagePackDecoding(#[from] rmp_serde::decode::Error),
}

pub fn encode<T: Serialize>(data: T) -> Result<String, EncoderError> {
    let encoded = rmp_serde::to_vec(&data)?;
    Ok(BASE64_STANDARD_NO_PAD.encode(encoded))
}

pub fn decode<T: DeserializeOwned>(from: &str) -> Result<T, EncoderError> {
    let decoded = BASE64_STANDARD_NO_PAD.decode(from)?;
    Ok(rmp_serde::from_slice(&decoded)?)
}
