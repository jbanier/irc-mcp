mod client;
pub mod dcc;
pub mod zip;
pub mod sasl;

pub use client::{start_message_processor, IrcClientManager};
pub use dcc::{download_dcc_file, parse_dcc_send};
