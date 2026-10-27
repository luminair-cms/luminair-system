//! Codecs for zero-clone HTTP request decoding and streaming response encoding.

mod request;
mod response;

pub use request::{
    DecodedPayload, decode_document_payload, decode_fields_map, decode_relations_map,
};
pub use response::*;
