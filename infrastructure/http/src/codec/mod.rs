//! Codecs for zero-clone HTTP request decoding and streaming response encoding.

mod request;
mod response;

pub(crate) use request::decode_document_payload;
pub(crate) use response::*;
