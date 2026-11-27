pub mod anthropic;
mod history;
pub use history::image_policy;
pub mod network;
pub mod usage;
pub mod openai_compat;
pub mod scripted;
mod stream;
#[cfg(test)]
mod tests;
#[derive(Default)]
pub(super) struct Reply {
    blocks: Vec<serde_json::Value>,
    reason: String,
    usage: serde_json::Value,
}
